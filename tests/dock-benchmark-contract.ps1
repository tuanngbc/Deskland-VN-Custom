$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) ('deskland-counter-contract-' + [Guid]::NewGuid())
$cases = @('control','dock') | ForEach-Object {
    (New-Item -ItemType Directory -Path (Join-Path $root $_)).FullName
}
$summary = Join-Path $PSScriptRoot 'summarize-dock-controlled.ps1'
$manifest = [ordered]@{
    Schema=2;State='VALID_INTERVAL';Build=('a'*40);Requested=125;Actual=125;
    WarmupSeconds=15;LogicalCpus=4;Adapters=@(@{Name='Fixture GPU';DriverVersion='1'});
    Display='fixture-display';Coverage='fixture-coverage';ObservedCoverage='fixture-coverage';Theme='beach';Mode='panorama';SceneSeed=32600;
    ControlSettingsSha256=('A'*64);FixtureRect='-300,100,100,330';FixtureFullscreen='none';
    PreviewFixtureCards=0;SessionWebViewCount=0;CounterWarnings=0
}
function Write-Manifest($dir) {
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $dir 'verdict.json')
}
function Expect-Invalid($match) {
    try {
        $null = & $summary -Baseline $cases[0] -Candidate $cases[1]
    } catch {
        if ($_.Exception.Message -match $match) { return }
        throw
    }
    throw "Comparison accepted an invalid pair ($match)"
}

try {
    foreach ($dir in $cases) {
        $load = if ($dir -eq $cases[0]) { 2.5 } else { 3.5 }
        $rows = 0..124 | ForEach-Object {
            [pscustomobject]@{Sequence=$_;Epoch=1000+$_;CpuSeconds=$_/25.0;DwmCPU=1;
                PrivateBytes=50MB;WorkingSet=40MB;Handles=120;GDI=20;USER=10}
        }
        $rows | Export-Csv -LiteralPath (Join-Path $dir 'controlled-process.csv') -NoTypeInformation
        $engines = 0..124 | ForEach-Object {
            [pscustomobject]@{Sequence=$_;Role='Deskland';Engine='adapter-a 3D';Value=$load;Status=0}
            [pscustomobject]@{Sequence=$_;Role='DWM';Engine='adapter-a 3D';Value=4;Status=0}
            [pscustomobject]@{Sequence=$_;Role='Deskland';Engine='adapter-b copy';Value=9;Status=0}
        }
        $engines | Export-Csv -LiteralPath (Join-Path $dir 'controlled-engines.csv') -NoTypeInformation
        @(
            '[1005.000] RAM riêng 50.0 MB, working set 40.0 MB | 90.0 khung/giây, CPU vẽ 9.20 ms/khung'
            '[1020.000] RAM riêng 50.0 MB, working set 40.0 MB | 30.0 khung/giây, CPU vẽ 1.80 ms/khung'
            '[1125.000] RAM riêng 50.0 MB, working set 40.0 MB | 90.0 khung/giây, CPU vẽ 9.20 ms/khung'
        ) | Set-Content -LiteralPath (Join-Path $dir 'tuanpc-desk.log') -Encoding utf8
        Write-Manifest $dir
    }
    $result = & $summary -Baseline $cases[0] -Candidate $cases[1] | ConvertFrom-Json
    if ($result.State -cne 'VALID_PAIR' -or $result.PerEngineGPU.'Deskland, adapter-a 3D'.MedianDelta -ne 1 -or
        $result.PerEngineGPU.'DWM, adapter-a 3D'.MedianDelta -ne 0 -or
        $result.PerEngineGPU.'Deskland, adapter-b copy'.MedianDelta -ne 0) { throw 'Matched engine contract failed' }
    if ($result.WallpaperFrameWindows.Baseline.Count -ne 1 -or
        $result.WallpaperFrameWindows.Baseline.MedianWindowFps -ne 30 -or
        $result.WallpaperFrameWindows.Baseline.MedianWindowMeanClientMs -ne 1.8) { throw 'Wallpaper frame sample exceeded measurement interval' }
    $manifest.ControlSettingsSha256 = 'B'*64
    Write-Manifest $cases[1]
    Expect-Invalid 'ControlSettingsSha256 differs'
    $manifest.ControlSettingsSha256 = 'A'*64
    $manifest.SceneSeed = 79500
    Write-Manifest $cases[1]
    Expect-Invalid 'SceneSeed differs'
    $manifest.SceneSeed = 32600
    $manifest.ObservedCoverage = 'wrong-coverage'
    Write-Manifest $cases[1]
    Expect-Invalid 'missing control settings or fixture metadata'
    $manifest.ObservedCoverage = 'fixture-coverage'
    $manifest.FixtureRect = '-299,100,101,330'
    Write-Manifest $cases[1]
    Expect-Invalid 'FixtureRect differs'
    $manifest.FixtureRect = '-300,100,100,330'
    $manifest.State = 'INVALID'
    Write-Manifest $cases[1]
    Expect-Invalid 'INVALID interval:'
    $manifest.State = 'VALID_INTERVAL'
    Write-Manifest $cases[1]
    for ($i=60; $i -lt $rows.Count; $i++) { $rows[$i].Epoch++ }
    $rows | Export-Csv -LiteralPath (Join-Path $cases[1] 'controlled-process.csv') -NoTypeInformation
    Expect-Invalid 'noncontinuous sample timestamps'
    Write-Output 'Controlled benchmark comparison contract passed'
} finally {
    # Remove only the fixture files created above, without a recursive delete.
    foreach ($dir in $cases) {
        foreach ($name in 'verdict.json','controlled-process.csv','controlled-engines.csv','tuanpc-desk.log') {
            $path = Join-Path $dir $name
            if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
        }
        Remove-Item -LiteralPath $dir
    }
    Remove-Item -LiteralPath $root
}
