param(
    [Parameter(Mandatory)][string]$Baseline,
    [Parameter(Mandatory)][string]$Candidate
)
$ErrorActionPreference = 'Stop'
function Percentile($values,[double]$p) {
    $ordered = @($values | Sort-Object)
    if (!$ordered.Count) { return $null }
    $ordered[[int][Math]::Max(0,[Math]::Ceiling($p*$ordered.Count)-1)]
}
function WallpaperFrameWindows($path,[double]$start,[double]$end) {
    $log = Join-Path $path 'tuanpc-desk.log'
    if (!(Test-Path -LiteralPath $log -PathType Leaf)) { return $null }
    $windows = @(Get-Content -LiteralPath $log -Encoding utf8 | ForEach-Object {
        if ($_ -match '^\[(\d+\.\d+)\].+\| (\d+\.\d+) khung/giây, CPU vẽ (\d+\.\d+) ms/khung$') {
            $numbers = $Matches
            $stamp = [double]::Parse($numbers[1],[Globalization.CultureInfo]::InvariantCulture)
            if ($stamp - 10 -ge $start -and $stamp -le $end) {
                [pscustomobject]@{
                    Fps=[double]::Parse($numbers[2],[Globalization.CultureInfo]::InvariantCulture)
                    ClientMs=[double]::Parse($numbers[3],[Globalization.CultureInfo]::InvariantCulture)
                }
            }
        }
    })
    if (!$windows.Count) { return $null }
    [ordered]@{
        Count=$windows.Count
        MedianWindowFps=Percentile $windows.Fps .5
        MedianWindowMeanClientMs=Percentile $windows.ClientMs .5
    }
}
function ReadInterval($path) {
    $m = Get-Content -LiteralPath (Join-Path $path 'verdict.json') -Raw | ConvertFrom-Json
    if ($m.State -cne 'VALID_INTERVAL') { throw "INVALID interval: $($m.Reasons -join ', ')" }
    if ($m.Schema -ne 2 -or $m.ControlSettingsSha256 -cnotmatch '^[A-F0-9]{64}$' -or
        $m.FixtureRect -notmatch '^-?\d+,-?\d+,-?\d+,-?\d+$' -or $m.FixtureFullscreen -notin 'none','primary','secondary' -or
        $null -eq $m.SceneSeed -or $m.ObservedCoverage -cne $m.Coverage) {
        throw 'INVALID interval: missing control settings or fixture metadata'
    }
    if ($m.StageEnabled -and ($m.StageFixturePreviews -lt 1 -or $m.PreviewFixtureCards -lt 1)) {
        throw 'INVALID interval: Stage lacks verified DWM fixture links'
    }
    $rows = @(Import-Csv -LiteralPath (Join-Path $path 'controlled-process.csv'))
    if ($rows.Count -ne $m.Actual -or $rows.Count -ne $m.Requested -or $rows.Count -lt 125) { throw 'Incomplete process data' }
    for ($i=0; $i -lt $rows.Count; $i++) {
        if ($rows[$i].Sequence -cne [string]$i) { throw 'INVALID interval: missing or duplicate process sample' }
        if ($i) {
            $gap = [double]$rows[$i].Epoch - [double]$rows[$i-1].Epoch
            if ($gap -le 0 -or $gap -gt 1.75) { throw 'INVALID interval: noncontinuous sample timestamps' }
        }
    }
    $time = [double]$rows[-1].Epoch - [double]$rows[0].Epoch
    if ($time -lt 120) { throw 'Interval shorter than 120 seconds' }
    $start = $rows[0]
    $end = $rows[-1]
    $process = [ordered]@{
        DesklandCpu = ([double]$end.CpuSeconds - [double]$start.CpuSeconds) * 100 / ($time * $m.LogicalCpus)
        DwmCpuMedian = Percentile ($rows | ForEach-Object {[double]$_.DwmCPU}) 0.5
        PrivateMiB = (Percentile ($rows | ForEach-Object {[double]$_.PrivateBytes}) 0.5) / 1MB
        WorkingSetMiB = (Percentile ($rows | ForEach-Object {[double]$_.WorkingSet}) 0.5) / 1MB
        Handles = Percentile ($rows | ForEach-Object {[int]$_.Handles}) 0.5
        GdiHandles = Percentile ($rows | ForEach-Object {[int]$_.GDI}) 0.5
        UserHandles = Percentile ($rows | ForEach-Object {[int]$_.USER}) 0.5
    }
    $gpu = [ordered]@{}
    $groups = Import-Csv -LiteralPath (Join-Path $path 'controlled-engines.csv') |
        Where-Object Status -eq '0' | Group-Object Role,Engine
    foreach ($group in $groups) {
        if ($group.Count -ne $m.Actual -or @($group.Group.Sequence | Sort-Object -Unique).Count -ne $m.Actual) { continue }
        $vals = @($group.Group | ForEach-Object {[double]$_.Value})
        $gpu[$group.Name] = [ordered]@{Median=(Percentile $vals .5);P95=(Percentile $vals .95);P99=(Percentile $vals .99)}
    }
    $traces = @(Get-ChildItem -LiteralPath $path -Filter 'dock-frames-*.csv' -File)
    $frame = $null
    if ($traces.Count -eq 1) {
        $frames = @(Import-Csv -LiteralPath $traces[0].FullName)
        $frame = @($frames | Group-Object output | ForEach-Object {
            $gaps = @($_.Group | Where-Object submission_gap_us | ForEach-Object {[double]$_.submission_gap_us/1000.0})
            $paint = @($_.Group | ForEach-Object {[double]$_.paint_us/1000.0})
            [ordered]@{Output=$_.Name;Submissions=$_.Count;CadenceMs=@((Percentile $gaps .5),(Percentile $gaps .95),(Percentile $gaps .99));PaintMs=@((Percentile $paint .5),(Percentile $paint .95),(Percentile $paint .99))}
        })
    }
    @{Metadata=$m;Process=$process;GPU=$gpu;Frame=$frame;Duration=$time;
        WallpaperFrames=(WallpaperFrameWindows $path ([double]$start.Epoch) ([double]$end.Epoch))}
}
$a = ReadInterval $Baseline
$b = ReadInterval $Candidate
if (($a.Metadata.StageEnabled -or $b.Metadata.StageEnabled) -and
    ($null -eq $a.Metadata.StageFixturePreviews -or $null -eq $b.Metadata.StageFixturePreviews)) {
    throw 'INVALID pair: both Stage workloads must be verified'
}
foreach ($field in 'Display','Coverage','Theme','Mode','SceneSeed','ControlSettingsSha256','FixtureRect','FixtureFullscreen','PreviewFixtureCards','SessionWebViewCount','LogicalCpus','WarmupSeconds','Requested') {
    if ($a.Metadata.$field -cne $b.Metadata.$field) { throw "INVALID pair: $field differs" }
}
if (($a.Metadata.Adapters | ConvertTo-Json -Compress) -cne ($b.Metadata.Adapters | ConvertTo-Json -Compress)) {
    throw 'INVALID pair: GPU adapters or drivers differ'
}
$metrics = [ordered]@{}
foreach ($name in $a.Process.Keys) {
    $metrics[$name] = [ordered]@{
        Baseline=$a.Process[$name];Candidate=$b.Process[$name];Delta=$b.Process[$name]-$a.Process[$name]
    }
}
$engines = [ordered]@{}
foreach ($key in $a.GPU.Keys) {
    if (!$b.GPU.Contains($key)) { continue }
    $engines[$key] = [ordered]@{Baseline=$a.GPU[$key];Candidate=$b.GPU[$key];MedianDelta=$b.GPU[$key].Median-$a.GPU[$key].Median}
}
foreach ($role in 'Deskland','DWM') {
    if (!@($engines.Keys | Where-Object { $_ -like "$role, *" }).Count) { throw "INVALID pair: no identical complete $role engine" }
}
[ordered]@{
    State='VALID_PAIR';BaselineBuild=$a.Metadata.Build;CandidateBuild=$b.Metadata.Build;
    BaselineSeconds=$a.Duration;CandidateSeconds=$b.Duration;Process=$metrics;PerEngineGPU=$engines;
    BaselineCounterWarnings=$a.Metadata.CounterWarnings;CandidateCounterWarnings=$b.Metadata.CounterWarnings;
    WallpaperFrameWindows=@{Baseline=$a.WallpaperFrames;Candidate=$b.WallpaperFrames};
    StageFixturePreviews=@{Baseline=$a.Metadata.StageFixturePreviews;Candidate=$b.Metadata.StageFixturePreviews};
    CandidateAnimationTrialLifetimeTrace=$b.Frame
} | ConvertTo-Json -Depth 10
