param(
    [Parameter(Mandatory)][string]$TrialDir,
    [Parameter(Mandatory)][string]$Evidence,
    [Parameter(Mandatory)][string]$BuildSha,
    [Parameter(Mandatory)][int]$CandidateId,
    [Parameter(Mandatory)][int]$SceneSeed,
    [ValidatePattern('^[A-F0-9]{64}$')][string]$ExpectedControlSettingsHash,
    [ValidateRange(0,16)][int]$ExpectedStagePreviews,
    [string]$AbortMarker,
    [string]$FixtureEvidence = $Evidence,
    [long]$SharedForegroundHwnd,
    [ValidateRange(15,600)][int]$Warmup = 15,
    [Alias('Samples')][ValidateRange(125,600)][int]$RequestedSamples = 125
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'dock-stage-workload.ps1')
if ($BuildSha -cnotmatch '^[a-f0-9]{40}$') { throw 'Use the complete GitHub build commit' }
$trial = (Resolve-Path -LiteralPath $TrialDir).Path
if ((Get-Content -LiteralPath (Join-Path $trial '.tuanpc-desk-trial') -Raw).Trim() -cne 'tuanpc-desk isolated native smoke profile v1') { throw 'Trial marker mismatch' }
if ((Get-Content -LiteralPath (Join-Path $trial 'dock-benchmark.enable') -Raw).Trim() -cne 'tuanpc-desk dock benchmark v1') { throw 'Benchmark marker mismatch' }
$candidate = Get-CimInstance Win32_Process -Filter "ProcessId=$CandidateId"
if (!$candidate -or $candidate.Name -ne 'tuanpc-desk.exe' -or !$candidate.CommandLine.Contains('--trial-dir') -or !$candidate.CommandLine.Contains($trial)) { throw 'Candidate ownership mismatch' }
$build = Split-Path -Parent $candidate.ExecutablePath
if ((Get-Content -LiteralPath (Join-Path $build 'BUILD-COMMIT.txt') -Raw).Trim() -cne $BuildSha) { throw 'Candidate build mismatch' }
if (Test-Path -LiteralPath (Join-Path $Evidence 'verdict.json')) { throw 'Select an unused evidence directory' }
$fixture = Get-Content -LiteralPath (Join-Path $FixtureEvidence 'fixture-ready.json') -Raw | ConvertFrom-Json
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
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr FindWindowExW(IntPtr parent, IntPtr after, string cls, string title);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    public static bool PhysicalWindowRect(IntPtr window, out Rect rect) {
        var previous = SetThreadDpiAwarenessContext((IntPtr)(-4));
        if (previous == IntPtr.Zero) { rect = default(Rect); return false; }
        try { return GetWindowRect(window, out rect); }
        finally { SetThreadDpiAwarenessContext(previous); }
    }
    public static bool SettingsVisible(uint expectedPid) {
        foreach (var cls in new[] {"TuanpcDeskSettings", "TuanpcDeskWebSettings"}) {
            var win = IntPtr.Zero;
            while ((win = FindWindowExW(IntPtr.Zero, win, cls, null)) != IntPtr.Zero) {
                GetWindowThreadProcessId(win, out uint pid);
                if (pid == expectedPid && IsWindowVisible(win)) return true;
            }
        }
        return false;
    }
    public static string DisplaySnapshot() {
        var previous = SetThreadDpiAwarenessContext((IntPtr)(-4));
        if (previous == IntPtr.Zero) return null;
        try { return ReadDisplays(); }
        finally { SetThreadDpiAwarenessContext(previous); }
    }
    static string ReadDisplays() {
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
$focusRecord = $fixture
$focusDeactivationLog = 'fixture-deactivated.log'
if ($PSBoundParameters.ContainsKey('SharedForegroundHwnd')) {
    $focus = @($fixture.ForegroundWindows | Where-Object Hwnd -eq $SharedForegroundHwnd)
    if ($focus.Count -ne 1 -or $focus[0].DeactivationLog -cnotmatch '^(fixture|source-[0-7])-deactivated\.log$') {
        throw 'Shared foreground must be one known fixture window'
    }
    $focusRecord = $focus[0]
    $focusDeactivationLog = $focusRecord.DeactivationLog
}
$fixtureHwnd = [IntPtr][int64]$focusRecord.Hwnd
[uint32]$owner = 0
[void][DeskPerfCounters]::GetWindowThreadProcessId($fixtureHwnd,[ref]$owner)
if ($owner -ne $fixtureProcess.Id) { throw 'Fixture HWND ownership mismatch' }
$log = Join-Path $build 'tuanpc-desk.log'
$settings = Join-Path $trial 'settings.json'
$settingsHash = (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash
$cfg = Get-Content -LiteralPath $settings -Raw | ConvertFrom-Json
$verifyStage = $PSBoundParameters.ContainsKey('ExpectedStagePreviews')
$controlCfg = Get-Content -LiteralPath $settings -Raw | ConvertFrom-Json
$controlCfg.PSObject.Properties.Remove('dock')
$controlJson = $controlCfg | ConvertTo-Json -Depth 100 -Compress -WarningAction Stop
$sha256 = [Security.Cryptography.SHA256]::Create()
try {
    $controlHash = [BitConverter]::ToString($sha256.ComputeHash([Text.Encoding]::UTF8.GetBytes($controlJson))).Replace('-','')
    $dockJson = $cfg.dock | ConvertTo-Json -Depth 100 -Compress -WarningAction Stop
    $dockCopy = $dockJson | ConvertFrom-Json
    foreach ($key in 'stageEnabled','stageDisplay','stageSide','stageStyles') { $dockCopy.PSObject.Properties.Remove($key) }
    $nonStageJson = $dockCopy | ConvertTo-Json -Depth 100 -Compress -WarningAction Stop
    $nonStageHash = [BitConverter]::ToString($sha256.ComputeHash([Text.Encoding]::UTF8.GetBytes($nonStageJson))).Replace('-','')
} finally { $sha256.Dispose() }
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
    $before.State
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
$preflightCounterErrors = $null
$observedCoverage = $null
$observedStagePreviews = $null
$stageSourceGeometry = $null
$lostFixtureFocus = $false
$observedSeed = if ($cfg.params -and $cfg.theme) { $cfg.params.PSObject.Properties[$cfg.theme].Value.seed }
try {
    if (!$cfg.wallpaperEnabled -or $null -eq $observedSeed -or $observedSeed -ne $SceneSeed -or
        $cfg.environment.time -ne $false -or $cfg.environment.weather -ne $false) {
        $reasons.Add('configuration-must-have-wallpaper-fixed-scene-and-static-time-weather')
    }
    if ($ExpectedControlSettingsHash -and $ExpectedControlSettingsHash -cne $controlHash) { $reasons.Add('control-settings-differ-from-first-interval') }
    if ($cfg.dock.stageEnabled -and !$verifyStage) { $reasons.Add('stage-workload-not-requested') }
    if ($verifyStage) {
        $stageMarker = Join-Path $trial 'dock-benchmark-stage.txt'
        $marker = if (Test-Path -LiteralPath $stageMarker -PathType Leaf) { (Get-Content -LiteralPath $stageMarker -Raw).Trim() }
        if ($marker -cne "$($fixture.PID) $($fixture.Hwnd)" -or $fixture.Cards -lt 1) { $reasons.Add('stage-fixture-marker-mismatch') }
        if ([bool]$cfg.dock.stageEnabled -ne ($ExpectedStagePreviews -gt 0)) { $reasons.Add('stage-settings-workload-mismatch') }
        if (!$cfg.dock.enabled -or $cfg.dock.allWindows -ne $true -or $cfg.dock.stageDisplay.mode -cne 'all' -or
            ($cfg.dock.stageEnabled -and $ExpectedStagePreviews -ne 2*$fixture.Cards)) {
            $reasons.Add('stage-requires-all-verified-fixture-cards-on-both-displays')
        }
        $sourceCheck = Join-Path $FixtureEvidence 'fixture-card-check.json'
        $sources = if (Test-Path -LiteralPath $sourceCheck -PathType Leaf) { Get-Content -LiteralPath $sourceCheck -Raw | ConvertFrom-Json }
        $ordinalSet = @($sources | Where-Object { $null -ne $_ } | Select-Object -ExpandProperty Card | Sort-Object)
        if (($ordinalSet -join ',') -cne ((0..($fixture.Cards-1)) -join ',') -or
            @($sources | Where-Object { $_.Visible -and !$_.Minimized -and $_.Unowned -and !$_.ToolWindow -and
                $_.CloakOk -and $_.Cloaked -eq 0 -and $_.AffinityOk -and $_.Affinity -eq 0 -and
                $_.Output -in 'primary','secondary' -and $_.Rect -match '^-?\d+,-?\d+,-?\d+,-?\d+$' }).Count -ne $fixture.Cards) {
            $reasons.Add('stage-source-model-not-verified')
        }else{
            $stageSourceGeometry = ($sources | Sort-Object Card | ForEach-Object { "$($_.Output):$($_.Rect)" }) -join ';'
        }
    }
    Start-Sleep -Seconds $Warmup
    $display = [DeskPerfCounters]::DisplaySnapshot()
    $web = (SessionWebView) -join ','
    if ([DeskPerfCounters]::GetForegroundWindow() -ne $fixtureHwnd) { $reasons.Add('fixture-not-foreground') }
    if (![DeskPerfCounters]::PhysicalWindowRect($fixtureHwnd,[ref]$anchor)) { $reasons.Add('fixture-window-gone') }
    if ($anchor.ToString() -cne $focusRecord.Rect) { $reasons.Add('fixture-not-in-declared-geometry') }
    if ([DeskPerfCounters]::SettingsVisible($CandidateId)) { $reasons.Add('candidate-settings-visible') }
    if (!$display) { $reasons.Add('two-display-geometry-or-dpi-unavailable') }
    $preflightTime = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0
    $observedCoverage = AssertCoverage $preflightTime $preflightTime
    if ($verifyStage -and !(Test-DockStageWorkload -Lines (Get-Content -LiteralPath $log -Encoding utf8) -From $preflightTime -Through $preflightTime -Expected $ExpectedStagePreviews)) {
        $reasons.Add('unverified-initial-stage-workload')
    }
    if (!$reasons.Count) {
        # Check real PDH data, rather than just whether the counter set exists.
        $check = Get-Counter -Counter $counter -SampleInterval 1 -MaxSamples 2 -ErrorAction SilentlyContinue -ErrorVariable +preflightCounterErrors |
            Select-Object -Last 1
        foreach ($id in $CandidateId,$dwmId) {
            $prefix = '^pid_' + $id + '_luid_[a-f0-9x]+_[a-f0-9x]+_phys_\d+_eng_\d+_engtype_.+$'
            if (!@($check.CounterSamples | Where-Object {$_.Status -eq 0 -and $_.InstanceName -match $prefix -and
                      ![double]::IsInfinity($_.CookedValue) -and $_.CookedValue -ge 0}).Count) {
                $reasons.Add($(if ($id -eq $CandidateId) {'preflight-deskland-gpu-unavailable'}else{'preflight-dwm-gpu-unavailable'}))
            }
        }
        $started = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0
        $observedCoverage = AssertCoverage $preflightTime $started
        if ([DeskPerfCounters]::GetForegroundWindow() -ne $fixtureHwnd) { $reasons.Add('fixture-lost-foreground-during-preflight') }
        if ((Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -cne $settingsHash) { $reasons.Add('trial-settings-changed-during-preflight') }
    }
    if (!$reasons.Count) {
        Get-Counter -Counter $counter -SampleInterval 1 -MaxSamples $RequestedSamples -ErrorAction SilentlyContinue -ErrorVariable +counterErrors | ForEach-Object {
            $pdh = $_.CounterSamples
            $utc = [DateTimeOffset]$_.Timestamp
            $epoch = $utc.ToUnixTimeMilliseconds() / 1000.0
            $i = $samples.Count
            if ($AbortMarker -and (Test-Path -LiteralPath $AbortMarker -PathType Leaf)) { throw [OperationCanceledException]::new('Controlled test canceled') }
            if ($app.HasExited) { $reasons.Add("candidate-exited:$i"); return }
            $app.Refresh()
            if ([DeskPerfCounters]::GetForegroundWindow() -ne $fixtureHwnd) {
                $reasons.Add("foreground-changed:$i")
                $lostFixtureFocus = $true
                throw [OperationCanceledException]::new('Fixture lost foreground; this interval cannot qualify')
            }
            [DeskPerfCounters+Rect]$now = [DeskPerfCounters+Rect]::new()
            if (![DeskPerfCounters]::PhysicalWindowRect($fixtureHwnd,[ref]$now) -or $now.ToString() -cne $anchor.ToString()) { $reasons.Add("fixture-geometry-changed:$i") }
            if ([DeskPerfCounters]::SettingsVisible($CandidateId)) { $reasons.Add("candidate-settings-opened:$i") }
            if ([DeskPerfCounters]::DisplaySnapshot() -cne $display) { $reasons.Add("display-geometry-or-dpi-changed:$i") }
            if (((SessionWebView) -join ',') -cne $web) { $reasons.Add("session-webview-set-changed:$i") }
            if ($i -and ($epoch - $samples[$i-1].Epoch -gt 1.75 -or $epoch -le $samples[$i-1].Epoch)) { $reasons.Add("missing-or-nonmonotonic-sample:$i") }
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
    if ($samples.Count -lt $RequestedSamples -or $samples[-1].Epoch-$samples[0].Epoch -lt 120) { $reasons.Add('less-than-120-continuous-seconds') }
    if ($samples.Count) {
        $null = AssertCoverage $started $samples[-1].Epoch
        if ($verifyStage) {
            if (Test-DockStageWorkload -Lines (Get-Content -LiteralPath $log -Encoding utf8) -From $preflightTime -Through $samples[-1].Epoch -Expected $ExpectedStagePreviews) {
                $observedStagePreviews = $ExpectedStagePreviews
            } else {
                $reasons.Add('stage-live-preview-workload-changed')
            }
        }
        $deactivated = Join-Path $FixtureEvidence $focusDeactivationLog
        if (Test-Path -LiteralPath $deactivated) {
            $lost = @(Get-Content -LiteralPath $deactivated | Where-Object {
                $_ -match '^\d+$' -and [long]$_ -ge $started*1000 -and [long]$_ -le $samples[-1].Epoch*1000
            })
            if ($lost.Count) { $reasons.Add('fixture-deactivated-between-samples') }
        }
    }
    if ((Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -cne $settingsHash) { $reasons.Add('trial-settings-changed') }
    foreach ($role in 'Deskland','DWM') {
        $valid = @($engines | Where-Object {$_.Role -eq $role -and $_.Status -eq 0} | Group-Object Engine | Where-Object Count -eq $RequestedSamples)
        if (!$valid.Count) { $reasons.Add("no-complete-GPU-series:$role") }
    }
} catch {
    if (!$lostFixtureFocus) {
        $reasons.Add("sampler-error:$($_.Exception.GetType().Name)")
        throw
    }
} finally {
    $samples | Export-Csv -LiteralPath (Join-Path $Evidence 'controlled-process.csv') -NoTypeInformation
    $engines | Export-Csv -LiteralPath (Join-Path $Evidence 'controlled-engines.csv') -NoTypeInformation
    [ordered]@{
        Schema=2;State=if($reasons.Count){'INVALID'}else{'VALID_INTERVAL'};Reasons=@($reasons.ToArray());Build=$BuildSha;
        WarmupSeconds=$Warmup;Requested=$RequestedSamples;Actual=$samples.Count;LogicalCpus=$logical;Adapters=$adapters;
        Display=$display;Coverage=$expected;ObservedCoverage=$observedCoverage;Theme=$cfg.theme;Mode=$cfg.mode;ControlSettingsSha256=$controlHash;SceneSeed=$observedSeed;
        FixtureRect=$anchor.ToString();FixtureFocusRole=if($SharedForegroundHwnd){$focusDeactivationLog};FixtureFullscreen=$fixture.Fullscreen;PreviewFixtureCards=$fixture.Cards;
        StageFixturePreviews=$observedStagePreviews;StageSourceGeometry=$stageSourceGeometry;NonStageSettingsSha256=$nonStageHash;
        DockEnabled=[bool]$cfg.dock.enabled;StageEnabled=[bool]$cfg.dock.stageEnabled;SessionWebViewCount=if($web){($web -split ',').Count}else{0};
        CounterWarnings=$counterErrors.Count
        PreflightCounterWarnings=$preflightCounterErrors.Count
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $Evidence 'verdict.json')
}
