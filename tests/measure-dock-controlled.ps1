param(
    [Parameter(Mandatory)][string]$TrialDir,
    [Parameter(Mandatory)][string]$Evidence,
    [Parameter(Mandatory)][string]$BuildSha,
    [Parameter(Mandatory)][int]$CandidateId,
    [ValidateRange(15,600)][int]$Warmup = 15,
    [ValidateRange(125,600)][int]$Samples = 125
)
$ErrorActionPreference = 'Stop'
if ($BuildSha -cnotmatch '^[a-f0-9]{40}$') { throw 'Use the complete GitHub build commit' }
$trial = (Resolve-Path -LiteralPath $TrialDir).Path
if ((Get-Content -LiteralPath (Join-Path $trial '.tuanpc-desk-trial') -Raw).Trim() -cne 'tuanpc-desk isolated native smoke profile v1') { throw 'Trial marker mismatch' }
if ((Get-Content -LiteralPath (Join-Path $trial 'dock-benchmark.enable') -Raw).Trim() -cne 'tuanpc-desk dock benchmark v1') { throw 'Benchmark marker mismatch' }
$candidate = Get-CimInstance Win32_Process -Filter "ProcessId=$CandidateId"
if (!$candidate -or $candidate.Name -ne 'tuanpc-desk.exe' -or !$candidate.CommandLine.Contains('--trial-dir') -or !$candidate.CommandLine.Contains($trial)) { throw 'Candidate ownership mismatch' }
$build = Split-Path -Parent $candidate.ExecutablePath
if ((Get-Content -LiteralPath (Join-Path $build 'BUILD-COMMIT.txt') -Raw).Trim() -cne $BuildSha) { throw 'Candidate build mismatch' }
if (Test-Path -LiteralPath (Join-Path $Evidence 'verdict.json')) { throw 'Select an unused evidence directory' }
$fixture = Get-Content -LiteralPath (Join-Path $Evidence 'fixture-ready.json') -Raw | ConvertFrom-Json
$fixtureProcess = Get-Process -Id $fixture.PID
$expected = [string]$fixture.Coverage
if ($expected -notmatch '^-?\d+,-?\d+,-?\d+,-?\d+:[01];-?\d+,-?\d+,-?\d+,-?\d+:[01]$') { throw 'Two-output fixture coverage contract mismatch' }
if (!('DeskPerfCounters' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class DeskPerfCounters {
    [StructLayout(LayoutKind.Sequential)] public struct Rect {
        public int l,t,r,b;
        public override string ToString() { return $"{l},{t},{r},{b}"; }
    }
    [StructLayout(LayoutKind.Sequential)] struct Monitor {
        public uint size;
        public Rect bounds, work;
        public uint flags;
    }
    delegate bool Visit(IntPtr monitor, IntPtr dc, ref Rect rect, IntPtr state);
    [DllImport("user32.dll")] static extern bool EnumDisplayMonitors(IntPtr dc, IntPtr clip, Visit visit, IntPtr state);
    [DllImport("user32.dll")] static extern bool GetMonitorInfoW(IntPtr monitor, ref Monitor info);
    [DllImport("shcore.dll")] static extern int GetDpiForMonitor(IntPtr monitor, uint type, out uint x, out uint y);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr process, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    public static string DisplaySnapshot() {
        var all = new List<string>();
        bool bad = false;
        bool ok = EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, (IntPtr h, IntPtr dc, ref Rect bounds, IntPtr data) => {
            var info = new Monitor { size = (uint)Marshal.SizeOf<Monitor>() };
            if (!GetMonitorInfoW(h, ref info) || GetDpiForMonitor(h, 0, out uint x, out uint y) != 0 || x == 0 || y == 0) {
                bad = true; return false;
            }
            all.Add($"{info.bounds}/{info.work}/{x}x{y}/{info.flags}");
            return true;
        }, IntPtr.Zero);
        if (!ok || bad || all.Count != 2) return null;
        all.Sort(StringComparer.Ordinal);
        return String.Join(";", all);
    }
}
'@
}
$fixtureHwnd = [IntPtr][int64]$fixture.Hwnd
[uint32]$owner = 0
[void][DeskPerfCounters]::GetWindowThreadProcessId($fixtureHwnd,[ref]$owner)
if ($owner -ne $fixtureProcess.Id) { throw 'Fixture HWND ownership mismatch' }
$log = Join-Path $build 'tuanpc-desk.log'
$settings = Join-Path $trial 'settings.json'
$settingsHash = (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash
$cfg = Get-Content -LiteralPath $settings -Raw | ConvertFrom-Json
$app = Get-Process -Id $CandidateId
$session = $app.SessionId
$dwm = @(Get-Process -Name dwm | Where-Object SessionId -eq $session)
if ($dwm.Count -ne 1) { throw 'Expected one DWM process in the candidate session' }
$dwmId = $dwm[0].Id

function SessionWebView {
    @(Get-Process -Name msedgewebview2 -ErrorAction SilentlyContinue |
        Where-Object SessionId -eq $session | Select-Object -ExpandProperty Id | Sort-Object)
}
function AssertCoverage($from,$through) {
    $coverage = @(Get-Content -LiteralPath $log -Encoding utf8 | ForEach-Object {
        if ($_ -match '^\[(\d+\.\d+)\] DOCK_BENCH_COVERAGE (.*)$') {
            [pscustomobject]@{Timestamp=[double]::Parse($Matches[1],[Globalization.CultureInfo]::InvariantCulture);State=$Matches[2]}
        }
    })
    $before = $coverage | Where-Object Timestamp -le $from | Select-Object -Last 1
    if (!$before -or $before.State -cne $expected) { $reasons.Add('unverified-initial-wallpaper-coverage') }
    if (@($coverage | Where-Object {$_.Timestamp -gt $from -and $_.Timestamp -le $through -and $_.State -cne $expected}).Count) {
        $reasons.Add('wallpaper-coverage-changed')
    }
}

$counter = @('\GPU Engine(*)\Utilization Percentage','\Process(dwm*)\ID Process','\Process(dwm*)\% Processor Time')
$logical = (Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors
$adapters = @(Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion)
$samples = [Collections.Generic.List[object]]::new()
$engines = [Collections.Generic.List[object]]::new()
$reasons = [Collections.Generic.List[string]]::new()
[DeskPerfCounters+Rect]$anchor = [DeskPerfCounters+Rect]::new()
$display = $null
$web = $null
$started = $null
$counterErrors = $null
try {
    Start-Sleep -Seconds $Warmup
    $display = [DeskPerfCounters]::DisplaySnapshot()
    $web = (SessionWebView) -join ','
    $started = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0
    if ([DeskPerfCounters]::GetForegroundWindow() -ne $fixtureHwnd) { $reasons.Add('fixture-not-foreground') }
    if (![DeskPerfCounters]::GetWindowRect($fixtureHwnd,[ref]$anchor)) { $reasons.Add('fixture-window-gone') }
    if (!$display) { $reasons.Add('two-display-geometry-or-dpi-unavailable') }
    AssertCoverage $started $started
    if (!$reasons.Count) {
        Get-Counter -Counter $counter -SampleInterval 1 -MaxSamples $Samples -ErrorAction Continue -ErrorVariable +counterErrors | ForEach-Object {
            $pdh = $_.CounterSamples
            $utc = [DateTimeOffset]$_.Timestamp
            $epoch = $utc.ToUnixTimeMilliseconds() / 1000.0
            $i = $samples.Count
            if ($app.HasExited) { $reasons.Add("candidate-exited:$i"); return }
            $app.Refresh()
            if ([DeskPerfCounters]::GetForegroundWindow() -ne $fixtureHwnd) { $reasons.Add("foreground-changed:$i") }
            [DeskPerfCounters+Rect]$now = [DeskPerfCounters+Rect]::new()
            if (![DeskPerfCounters]::GetWindowRect($fixtureHwnd,[ref]$now) -or $now.ToString() -cne $anchor.ToString()) { $reasons.Add("fixture-geometry-changed:$i") }
            if ([DeskPerfCounters]::DisplaySnapshot() -cne $display) { $reasons.Add("display-geometry-or-dpi-changed:$i") }
            if (((SessionWebView) -join ',') -cne $web) { $reasons.Add("session-webview-set-changed:$i") }
            if ($i -and ($epoch - $samples[$i-1].Epoch -gt 2.5 -or $epoch -le $samples[$i-1].Epoch)) { $reasons.Add("missing-or-nonmonotonic-sample:$i") }
            $pd = $pdh | Where-Object {$_.Status -eq 0 -and $_.Path -match '\\id process$' -and [int]$_.CookedValue -eq $dwmId} | Select-Object -First 1
            $dwmCPU = if ($pd) { ($pdh | Where-Object {$_.Status -eq 0 -and $_.InstanceName -eq $pd.InstanceName -and $_.Path -match '\\% processor time$'} | Select-Object -First 1).CookedValue }
            if ($null -eq $dwmCPU) { $reasons.Add("missing-dwm-cpu:$i") }
            $samples.Add([pscustomobject]@{
                Sequence=$i;Epoch=$epoch;CpuSeconds=$app.CPU;DwmCPU=if($null -ne $dwmCPU){$dwmCPU/$logical};
                PrivateBytes=$app.PrivateMemorySize64;WorkingSet=$app.WorkingSet64;Handles=$app.HandleCount;
                GDI=[DeskPerfCounters]::GetGuiResources($app.Handle,0);USER=[DeskPerfCounters]::GetGuiResources($app.Handle,1)
            })
            foreach ($point in $pdh) {
                if ($point.InstanceName -match '^pid_(\d+)_(luid_[a-f0-9x]+_[a-f0-9x]+_phys_\d+_eng_\d+_engtype_.+)$') {
                    $id = [int]$Matches[1]
                    if ($id -eq $CandidateId -or $id -eq $dwmId) {
                        $engines.Add([pscustomobject]@{Sequence=$i;Role=if($id -eq $CandidateId){'Deskland'}else{'DWM'};Engine=$Matches[2];Value=$point.CookedValue;Status=$point.Status})
                    }
                }
            }
        }
    }
    if ($samples.Count -lt $Samples -or $samples[-1].Epoch-$samples[0].Epoch -lt 120) { $reasons.Add('less-than-120-continuous-seconds') }
    if ($samples.Count) {
        AssertCoverage $started $samples[-1].Epoch
        $deactivated = Join-Path $Evidence 'fixture-deactivated.log'
        if (Test-Path -LiteralPath $deactivated) {
            $lost = @(Get-Content -LiteralPath $deactivated | Where-Object {
                $_ -match '^\d+$' -and [long]$_ -ge $started*1000 -and [long]$_ -le $samples[-1].Epoch*1000
            })
            if ($lost.Count) { $reasons.Add('fixture-deactivated-between-samples') }
        }
    }
    if ((Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -cne $settingsHash) { $reasons.Add('trial-settings-changed') }
    foreach ($role in 'Deskland','DWM') {
        $valid = @($engines | Where-Object {$_.Role -eq $role -and $_.Status -eq 0} | Group-Object Engine | Where-Object Count -eq $Samples)
        if (!$valid.Count) { $reasons.Add("no-complete-GPU-series:$role") }
    }
} catch {
    $reasons.Add("sampler-error:$($_.Exception.GetType().Name)")
    throw
} finally {
    $samples | Export-Csv -LiteralPath (Join-Path $Evidence 'controlled-process.csv') -NoTypeInformation
    $engines | Export-Csv -LiteralPath (Join-Path $Evidence 'controlled-engines.csv') -NoTypeInformation
    [ordered]@{
        State=if($reasons.Count){'INVALID'}else{'VALID_INTERVAL'};Reasons=@($reasons.ToArray());Build=$BuildSha;
        WarmupSeconds=$Warmup;Requested=$Samples;Actual=$samples.Count;LogicalCpus=$logical;Adapters=$adapters;
        Display=$display;Coverage=$expected;Theme=$cfg.theme;Mode=$cfg.mode;PreviewFixtureCards=$fixture.Cards;
        DockEnabled=[bool]$cfg.dock.enabled;StageEnabled=[bool]$cfg.dock.stageEnabled;SessionWebViewCount=if($web){($web -split ',').Count}else{0};
        CounterWarnings=@($counterErrors).Count
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $Evidence 'verdict.json')
}
