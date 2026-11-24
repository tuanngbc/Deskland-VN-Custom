$ErrorActionPreference='Stop'
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'GitHub Actions only' }
. "$PSScriptRoot\..\tools\owner-uat.ps1" -LibraryOnly
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
