$ErrorActionPreference = 'Stop'
$evidence = Join-Path ([IO.Path]::GetTempPath()) ('deskland-card-smoke-' + [guid]::NewGuid())
$null = New-Item -ItemType Directory -Path $evidence
try {
    $fixture = Join-Path $PSScriptRoot 'dock-perf-fixture.ps1'
    $winps = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $proc = Start-Process -FilePath $winps -WindowStyle Hidden -PassThru -ArgumentList @(
        '-NoProfile','-STA','-File',('"' + $fixture + '"'),'-Evidence',('"' + $evidence + '"'),'-Cards','3','-SourceSmoke')
    if (!$proc.WaitForExit(20000)) {
        $proc.Kill()
        $proc.WaitForExit()
        throw 'Hidden-launch preview fixture timed out'
    }
    if ($proc.ExitCode -ne 0) { throw "Hidden-launch preview fixture failed: $($proc.ExitCode)" }
    $sources = Get-Content -LiteralPath (Join-Path $evidence 'fixture-card-check.json') -Raw | ConvertFrom-Json
    if (@($sources).Count -ne 3 -or @($sources | Where-Object Visible).Count -ne 3) {
        throw 'All three real source windows must be visible after hidden launch'
    }
    Write-Output "Native source smoke: three visible/eligible HWNDs; initially hidden: $(@($sources | Where-Object { !$_.InitiallyVisible }).Count)"
} finally {
    $check = Join-Path $evidence 'fixture-card-check.json'
    [IO.File]::Delete($check)
    foreach ($name in 'fixture-deactivated.log','source-0-deactivated.log','source-1-deactivated.log','source-2-deactivated.log') {
        [IO.File]::Delete((Join-Path $evidence $name))
    }
    [IO.Directory]::Delete($evidence, $false)
}
