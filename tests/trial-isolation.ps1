param([Parameter(Mandatory=$true)][string]$Exe)
$ErrorActionPreference='Stop'

function Snapshot-Persistence {
    $keys=@('Software\Microsoft\Windows\CurrentVersion\Run','Control Panel\Desktop',
        'Software\Microsoft\Windows\CurrentVersion\Policies\System','Software\tuanpc-desk-TrialProbe')
    $items=@(foreach($key in $keys) {
        $handle=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($key)
        if($null -eq $handle){[pscustomobject]@{Key=$key;Exists=$false};continue}
        try {foreach($name in ($handle.GetValueNames()|Sort-Object)) {
            [pscustomobject]@{Key=$key;Name=$name;Kind=$handle.GetValueKind($name).ToString();Value=$handle.GetValue($name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)}
        }} finally {$handle.Dispose()}
    })
    $profile=Join-Path $env:USERPROFILE '.deskland-rs'
    $files=@(if(Test-Path -LiteralPath $profile){Get-ChildItem -LiteralPath $profile -Recurse -Force -File | Sort-Object FullName | ForEach-Object {
        [pscustomobject]@{File=$_.FullName;Hash=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
    }})
    [pscustomobject]@{Registry=$items;Profile=$files}|ConvertTo-Json -Depth 10 -Compress
}
function Run-Check([string[]]$Arguments,[int]$Expected) {
    $process=Start-Process -FilePath $Exe -ArgumentList $Arguments -PassThru -WindowStyle Hidden
    if(-not $process.WaitForExit(30000)){$process.Kill();throw 'Trial isolation check timed out'}
    if($process.ExitCode -ne $Expected){throw "Trial check exit $($process.ExitCode), expected $Expected"}
}
$before=Snapshot-Persistence
$dir=Join-Path $env:RUNNER_TEMP ('tuanpc-desk-trial-ci-'+[Guid]::NewGuid().ToString('N'))
Run-Check @('--trial-isolation-check','--trial-dir',('"'+$dir+'"')) 0
if(-not(Test-Path -LiteralPath (Join-Path $dir 'ISOLATION-PASS.txt'))){throw 'Release EXE did not confirm its persistence guards'}
Set-Content -LiteralPath (Join-Path $dir 'settings.json') -Value '{"theme":"beach","widget":{"x":-800},"futureSentinel":73}' -Encoding utf8
Run-Check @('--trial-isolation-check','--trial-dir',('"'+$dir+'"')) 0
$value=Get-Content -LiteralPath (Join-Path $dir 'settings.json') -Raw | ConvertFrom-Json
if($value.futureSentinel -ne 73 -or $value.widget.x -ne -800 -or -not $value.trialProbe){throw 'Isolated profile write failed preservation'}
Run-Check @('--trial-isolation-check') 3
Run-Check @('--trial-dock-glass-probe') 2
Run-Check @('--trial-dock-displays') 2
Run-Check @('--trial-benchmark-coverage-probe') 2
Run-Check @('--trial-benchmark-coverage-probe','--trial-dir',('"'+$dir+'"')) 2
$probeSettings=(Get-FileHash -LiteralPath (Join-Path $dir 'settings.json')).Hash
Set-Content -LiteralPath (Join-Path $dir 'dock-benchmark.enable') -Value 'tuanpc-desk dock benchmark v1' -Encoding ascii
Run-Check @('--trial-benchmark-coverage-probe','--trial-dir',('"'+$dir+'"')) 0
$coverage=Get-Content -LiteralPath (Join-Path $dir 'wallpaper-coverage.json') -Raw | ConvertFrom-Json
if (!$coverage.outputs.Count -or @($coverage.outputs | Where-Object primary).Count -ne 1) { throw 'Native coverage probe supplied no primary output' }
foreach ($output in $coverage.outputs) {
    if ($output.rect.Count -ne 4 -or $output.rect[0] -ge $output.rect[2] -or $output.rect[1] -ge $output.rect[3] -or
        ($output.covered -ne ($output.reason -in @('maximized','bounds'))) -or
        (($output.PSObject.Properties.Name | Sort-Object) -join ',') -cne 'covered,primary,reason,rect') {
        throw 'Invalid or unredacted native coverage result'
    }
}
if ((Get-FileHash -LiteralPath (Join-Path $dir 'settings.json')).Hash -ne $probeSettings) { throw 'Coverage probe changed trial settings' }
Run-Check @('--trial-isolation-check','--trial-dir') 2
Run-Check @('--trial-isolation-check','--trial-dir','relative') 2
Run-Check @('--trial-isolation-check','--trial-dir',('"'+(Join-Path $env:USERPROFILE '.deskland-rs')+'"')) 2
$unowned=Join-Path $env:RUNNER_TEMP ('tuanpc-desk-trial-unowned-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $unowned | Out-Null
Set-Content -LiteralPath (Join-Path $unowned 'keep.txt') -Value 'unrelated data'
Run-Check @('--trial-isolation-check','--trial-dir',('"'+$unowned+'"')) 2
if((Snapshot-Persistence) -ne $before){throw 'Windows Registry or real Deskland profile changed during trial check'}
Write-Output 'TRIAL ISOLATION: release EXE guards and native coverage probe PASS; real Registry/profile unchanged; invalid/unowned paths refused.'
