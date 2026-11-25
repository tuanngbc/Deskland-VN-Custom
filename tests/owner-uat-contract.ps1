param([Parameter(Mandatory=$true)][string]$Exe)
$ErrorActionPreference='Stop'
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'GitHub Actions only' }
if ($PSVersionTable.PSVersion.Major -ne 5) { throw 'Run with Windows PowerShell 5.1, as used by the Desktop shortcut' }
$source=Join-Path $PSScriptRoot '..\tools\owner-uat.ps1'
. $source -LibraryOnly
$prod='C:\UAT-fixtures\production\TLauncher.exe'
$cand='C:\UAT-fixtures\candidate\TLauncher.exe'
function Fixture($Path,$Command,$Id) { [pscustomobject]@{ExecutablePath=$Path;CommandLine=('"'+$Path+'" '+$Command);ProcessId=$Id;CreationDate='sample'} }
function Must-Reject($Rows) { $rejected=$false; try { Get-LaunchPlan $Rows $prod $cand | Out-Null } catch {$rejected=$true}; if (-not $rejected) {throw 'Unsafe launch plan accepted'} }
$main=Fixture $prod '' 100
$guard=Fixture $prod '--dock-taskbar-guard 100 token' 101
$plan=Get-LaunchPlan @($main,$guard) $prod $cand
if (-not $plan.WasRunning -or $plan.Main.ProcessId -ne 100 -or $plan.Guards.Count -ne 1) {throw 'Incorrect main/guard classification'}
if ((Get-LaunchPlan @() $prod $cand).WasRunning) {throw 'Empty session should not restore production'}
Must-Reject @($guard)
Must-Reject @($main,(Fixture $prod '' 102))
Must-Reject @($main,(Fixture $cand '--trial-dir C:\fixture' 103))
Must-Reject @($main,(Fixture 'C:\Other\TLauncher.exe' '' 104))
Must-Reject @((Fixture $prod '--trial-dir C:\fixture' 105))
Must-Reject @([pscustomobject]@{ExecutablePath='';CommandLine='';ProcessId=106})
if (-not (Same-Path $prod 'c:\uat-fixtures\PRODUCTION\TLAUNCHER.EXE')) {throw 'Windows path identity mismatch'}
Write-Host 'OWNER_UAT_SWITCH_PLAN=PASS exact path, single main, guards, competing candidates, unknown identities'

& {
    $session=(Get-Process -Id $PID).SessionId
    $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $script:uatSnapshots=0
    $script:uatCase='transient'
    $same=[pscustomobject]@{ProcessId=100;SessionId=$session}
    $disappearing=[pscustomobject]@{ProcessId=101;SessionId=$session}
    function Get-CimInstance {
        param($ClassName,$Filter)
        $script:uatSnapshots++
        $same
        if ($script:uatSnapshots -eq 1 -and $script:uatCase -eq 'transient') { $disappearing }
    }
    function Invoke-CimMethod {
        param($InputObject,$MethodName)
        if ($script:uatCase -eq 'denied') { return [pscustomobject]@{ReturnValue=2;Sid=$null} }
        if ($script:uatCase -eq 'other') { return [pscustomobject]@{ReturnValue=0;Sid='S-1-0-0'} }
        if ($InputObject.ProcessId -eq 101) { throw 'Process disappeared' }
        [pscustomobject]@{ReturnValue=0;Sid=$sid}
    }
    function Start-Sleep { param($Milliseconds) }
    $snapshot=@(Get-SessionProcesses)
    if ($script:uatSnapshots -ne 2 -or $snapshot.Count -ne 1 -or $snapshot[0].ProcessId -ne 100) {
        throw 'An incomplete ownership snapshot was not discarded'
    }
    $script:uatSnapshots=0; $script:uatCase='denied'; $diagnostic=''
    try { Get-SessionProcesses | Out-Null } catch { $diagnostic=$_.Exception.Message }
    if ($script:uatSnapshots -ne 5 -or $diagnostic -notmatch 'GetOwnerSid returned 2') {
        throw 'Persistent ownership failure did not fail closed with a diagnostic'
    }
    $script:uatSnapshots=0; $script:uatCase='other'; $diagnostic=''
    try { Get-SessionProcesses | Out-Null } catch { $diagnostic=$_.Exception.Message }
    if ($script:uatSnapshots -ne 1 -or $diagnostic -notmatch 'different account') {
        throw 'A different owner was not rejected immediately'
    }
}
Write-Host 'OWNER_UAT_OWNERSHIP=PASS incomplete snapshots, persistent failure, foreign SID'

# Use the real shortcut target, executable metadata and on-disk manifest.
# Preflight cannot start or stop any executable.
$dir=Join-Path $env:RUNNER_TEMP ('owner-uat '+[guid]::NewGuid())
$root=Join-Path $dir 'TLauncher-UAT'
$trial=Join-Path $root 'candidate'
$profile=Join-Path $root 'profiles\tuanpc-desk-trial-ci'
New-Item -ItemType Directory -Path (Join-Path $trial 'assets\widget'),(Join-Path $trial 'prototype'),$profile | Out-Null
$target=Join-Path $trial 'TLauncher.exe'
$launcher=Join-Path $trial 'owner-uat.ps1'
Copy-Item -LiteralPath $Exe -Destination $target
Copy-Item -LiteralPath $source -Destination $launcher
$build='0123456789abcdef0123456789abcdef01234567'
Set-Content -LiteralPath (Join-Path $trial 'BUILD-COMMIT.txt') -Value $build -Encoding ascii
Set-Content -LiteralPath (Join-Path $trial 'VERSION.txt') -Value (Get-Item -LiteralPath $target).VersionInfo.ProductVersion -Encoding ascii
Set-Content -LiteralPath (Join-Path $trial 'prototype\settings.html') -Value '<!doctype html>' -Encoding ascii
$manifest=Join-Path $trial 'verified-artifact.json'
$data=@{sourceCommit=$build;exeSha256=(Get-FileHash -LiteralPath $target).Hash;profile=$profile;productionExe=$Exe}
$data | ConvertTo-Json | Set-Content -LiteralPath $manifest -Encoding UTF8
$ps51=Join-Path $PSHOME 'powershell.exe'
$output=& $ps51 -NoProfile -NonInteractive -STA -ExecutionPolicy Bypass -File $launcher -UseVerifiedManifest -PreflightOnly
if ($LASTEXITCODE -ne 0 -or $output -notcontains 'OWNER_UAT_PREFLIGHT=PASS') { throw 'Windows PowerShell shortcut preflight failed' }
if (Test-Path -LiteralPath (Join-Path $trial 'uat-state.json')) { throw 'Preflight entered the live switching path' }
Write-Host 'OWNER_UAT_SHORTCUT=PASS Windows PowerShell 5.1, manifest, spaces in path, relocated profile'
