param([Parameter(Mandatory=$true)][string]$Exe)
$ErrorActionPreference='Stop'
if($env:GITHUB_ACTIONS -cne 'true'){throw 'Use a disposable Windows Actions desktop for the Shell registration branch.'}
$dir=Join-Path $env:RUNNER_TEMP ('tuanpc-desk-trial-tray-'+[guid]::NewGuid().ToString('n'))
$null=New-Item -ItemType Directory -Path $dir
[IO.File]::WriteAllText((Join-Path $dir '.tuanpc-desk-trial'),"tuanpc-desk isolated native smoke profile v1"+[char]10)
$p=Start-Process -FilePath $Exe -ArgumentList @('--trial-dir',('"'+$dir+'"'),'--trial-tray-poc','--trial-tray-poc-shell') -WindowStyle Hidden -PassThru
if(!$p.WaitForExit(20000)){throw 'Cooperative tray prototype timed out.'}
if($p.ExitCode -ne 0){throw "Cooperative tray prototype exited $($p.ExitCode)."}
$file=Join-Path $dir 'cooperative-tray-poc.json'
$raw=Get-Content -LiteralPath $file -Raw
$r=$raw|ConvertFrom-Json
if(!$r.added -or $r.tooltipRevision -ne 2 -or $r.leftCallback -ne 1 -or $r.doubleCallback -ne 1 -or
   $r.appOwnedContextMenuItems -ne 2 -or !$r.removed -or !$r.cleanRestart -or !$r.recoveredAfterPeerExit -or $r.externalAppEnumeration){throw "Unexpected cooperative proof: $raw"}
Write-Output $raw
