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
Run-Check @('--trial-isolation-check','--trial-dir') 2
Run-Check @('--trial-isolation-check','--trial-dir','relative') 2
Run-Check @('--trial-isolation-check','--trial-dir',('"'+(Join-Path $env:USERPROFILE '.deskland-rs')+'"')) 2
$unowned=Join-Path $env:RUNNER_TEMP ('tuanpc-desk-trial-unowned-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $unowned | Out-Null
Set-Content -LiteralPath (Join-Path $unowned 'keep.txt') -Value 'unrelated data'
Run-Check @('--trial-isolation-check','--trial-dir',('"'+$unowned+'"')) 2
if((Snapshot-Persistence) -ne $before){throw 'Windows Registry or real Deskland profile changed during trial check'}
Write-Output 'TRIAL ISOLATION: release EXE guards PASS; real Registry/profile unchanged; invalid/unowned paths refused.'
