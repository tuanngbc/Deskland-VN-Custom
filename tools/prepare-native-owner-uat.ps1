# Download/verify/stage only. Never executes an EXE or alters a live candidate.
[CmdletBinding()]
param([Parameter(Mandatory)][long]$RunId,[Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{40}$')][string]$Commit,
 [string]$UatRoot='E:\TLauncher-UAT',[string]$ProductionExe='E:\TLauncher\TLauncher.exe',[switch]$Wallpaper)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$requestedProduction=$ProductionExe
. (Join-Path $PSScriptRoot 'legacy-uat-library.ps1') -LibraryOnly
$ProductionExe=$requestedProduction
$root=[IO.Path]::GetFullPath($UatRoot).TrimEnd('\')
if([IO.Path]::GetFileName($root)  -ine  'TLauncher-UAT'  -or  !(Test-Path $ProductionExe)  -or 
 (Get-Item $ProductionExe).VersionInfo.ProductVersion  -cne  '1.0.2'){throw 'Invalid UAT root or production'}
Assert-NoReparse $root
$kind=if($Wallpaper){'wallpaper'}else{'native-base'}
$candidate=Join-Path $root ($kind+'-1.0.3-'+$Commit.Substring(0,12))
$profile=Join-Path $root ('profiles\tlauncher-native-trial-'+$(if($Wallpaper){'wallpaper-'}else{''})+$Commit.Substring(0,12))
$download=Join-Path $root ($kind+'-downloads-'+$Commit.Substring(0,12))
foreach($path in @($candidate,$profile,$download,$ProductionExe)){Assert-NoReparse $path}
if(Test-Path -LiteralPath $candidate){throw 'Candidate exists; never change an existing or running session.'}
$stableHash=(Get-FileHash -LiteralPath $ProductionExe).Hash
$credentials=("protocol=https`nhost=github.com`n`n"|git credential fill)
$secret=$credentials|Where-Object {$_.StartsWith('password=')}|Select-Object -First 1
if(!$secret){throw 'GitHub repository credential unavailable'}
$headers=@{Authorization=('Bearer '+$secret.Substring(9));Accept='application/vnd.github+json';'X-GitHub-Api-Version'='2022-11-28'}
Remove-Variable credentials,secret
$api='https://api.github.com/repos/tuanngbc/TLauncher'
$run=Invoke-RestMethod "$api/actions/runs/$RunId" -Headers $headers
$expectedBranch=if($Wallpaper){'codex/native-wallpaper-reengineering-v1.0.3'}else{'codex/fluidwin-native-base-v1.0.3'}
if($run.head_sha  -cne  $Commit  -or  $run.head_branch  -cne  $expectedBranch  -or 
 $run.path  -cne  '.github/workflows/build-rust-test.yml'  -or  $run.conclusion  -cne  'success'  -or  $run.status  -cne  'completed'){throw 'Exact native workflow is not successful'}
$jobs=Invoke-RestMethod "$api/actions/runs/$RunId/jobs" -Headers $headers
$requiredJobs=@('fluidwin-baseline','native-base');if($Wallpaper){$requiredJobs+='wallpaper-contract'}
foreach($name in $requiredJobs){if(@($jobs.jobs|Where-Object {$_.name  -ceq  $name  -and  $_.conclusion  -ceq  'success'}).Count -ne  1){throw "Required job failed/missing: $name"}}
$artifacts=Invoke-RestMethod "$api/actions/runs/$RunId/artifacts" -Headers $headers
$art=@($artifacts.artifacts|Where-Object {$_.name  -ceq  'TLauncher-Native-Base-win-x64'  -and  !$_.expired})
if($art.Count -ne  1  -or  $art[0].workflow_run.head_sha  -cne  $Commit  -or  $art[0].digest  -cnotmatch  '^sha256:[a-f0-9]{64}$'){throw 'Native artifact missing/ambiguous'}
New-Item -ItemType Directory -Path $download -Force|Out-Null
$outer=Join-Path $download 'actions.zip'
if(!(Test-Path $outer) -or  ('sha256:'+(Get-FileHash $outer).Hash.ToLowerInvariant())  -cne  $art[0].digest){Invoke-WebRequest "$api/actions/artifacts/$($art[0].id)/zip" -Headers $headers -OutFile $outer}
$headers.Remove('Authorization')
if(('sha256:'+(Get-FileHash $outer).Hash.ToLowerInvariant())  -cne  $art[0].digest){throw 'GitHub archive digest mismatch'}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$name="TLauncher-Native-1.0.3-$Commit-win-x64"
$zip=Join-Path $download ($name+'.zip')
$container=[IO.Compression.ZipFile]::OpenRead($outer)
try{
 $image=@($container.Entries|Where-Object {$_.FullName -cin @(($name+'.zip'),('dist/'+$name+'.zip'))})
 $sum=@($container.Entries|Where-Object {$_.FullName -cin @(($name+'.zip.sha256'),('dist/'+$name+'.zip.sha256'))})
 if($image.Count -ne  1 -or  $sum.Count -ne  1 -or  $sum[0].Length -gt  256 -or  $image[0].Length -gt  150MB){throw 'Unexpected archive layout'}
 $reader=[IO.StreamReader]::new($sum[0].Open());try{$wanted=($reader.ReadToEnd().Trim()-split '\s+')[0]}finally{$reader.Dispose()}
 if($wanted -cnotmatch  '^[a-f0-9]{64}$'){throw 'Invalid checksum'}
 [IO.Compression.ZipFileExtensions]::ExtractToFile($image[0],$zip,$true)
}finally{$container.Dispose()}
if((Get-FileHash $zip).Hash.ToLowerInvariant() -cne  $wanted){throw 'Package SHA256 mismatch'}
$package=[IO.Compression.ZipFile]::OpenRead($zip)
$fileManifest=@()
try{
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase);$bytes=0L
 $members=@(foreach($entry in $package.Entries){
  $path=$entry.FullName.Replace('\','/')
  if($path -ceq  ($name+'/')){continue}
  if(!$path.StartsWith($name+'/',[StringComparison]::Ordinal)){throw 'Unexpected ZIP root'}
  $relative=$path.Substring($name.Length+1).TrimEnd('/')
  if(!$relative -or  $relative.Split('/')  -match  '^(?:\.|\.\.)$|[:<>|?*"]|[. ]$'){throw 'Unsafe ZIP member'}
  $to=[IO.Path]::GetFullPath((Join-Path $candidate $relative))
  if(!$to.StartsWith($candidate+'\',[StringComparison]::OrdinalIgnoreCase) -or  !$seen.Add($to) -or  (($entry.ExternalAttributes-shr 16)-band 0xF000) -eq  0xA000){throw 'Uncontained/linked ZIP member'}
  if($path.EndsWith('/')){continue}
  $bytes+=$entry.Length;if($bytes -gt  350MB -or  $seen.Count -gt  5000){throw 'Package exceeds budget'}
  Assert-NoReparse $to
  [pscustomobject]@{entry=$entry;to=$to;relative=$relative}
 })
 foreach($member in $members){
  [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($member.to))|Out-Null
  [IO.Compression.ZipFileExtensions]::ExtractToFile($member.entry,$member.to,$false)
  $fileManifest += [pscustomobject]@{path=$member.relative;sha256=(Get-FileHash -LiteralPath $member.to).Hash.ToLowerInvariant()}
 }
}finally{$package.Dispose()}
$exe=Join-Path $candidate 'TLauncher.exe'
if((Get-Content (Join-Path $candidate 'BUILD-COMMIT.txt') -Raw).Trim() -cne  $Commit -or 
 (Get-Content (Join-Path $candidate 'VERSION.txt') -Raw).Trim() -cne  '1.0.3' -or 
 (Get-Content (Join-Path $candidate 'UPSTREAM-COMMIT.txt') -Raw).Trim() -cne  '2f773a8733b84c7dbee88fff6628d14e9ac802b9' -or 
 (Get-Item $exe).VersionInfo.ProductVersion -cne  '1.0.3'){throw 'Candidate metadata mismatch'}
if($Wallpaper){
 if((Get-Content (Join-Path $candidate 'PACKAGE-KIND.txt') -Raw).Trim() -cne 'PHASE_2_WALLPAPER'){throw 'Not a Phase 2 wallpaper package'}
 $thumbs=@(Get-ChildItem (Join-Path $candidate 'wallpaper\thumbs') -Filter '*.png' -File);if($thumbs.Count -ne 16){throw 'Incomplete wallpaper gallery'}
 foreach($scene in 'meadow','beach','beach-turquoise','lake','sky','ocean','rainforest','mocchau','tet-sakura','cat-hill','stadium','aquarium','commute','summer-porch','cats','boxing-ring'){
  if(!(Test-Path (Join-Path $candidate ('wallpaper\thumbs\'+$scene+'.png')) -PathType Leaf)){throw "Missing thumbnail $scene"}
 }
 $assetRoot=Join-Path $candidate 'wallpaper'
 foreach($asset in (Get-Content (Join-Path $assetRoot 'ASSET-SHA256.json') -Raw|ConvertFrom-Json)){
  $path=[IO.Path]::GetFullPath((Join-Path $assetRoot $asset.path));if(!$path.StartsWith($assetRoot+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Uncontained asset manifest'}
  if((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $asset.sha256){throw "Wallpaper asset mismatch $($asset.path)"}
 }
 if(@(Get-ChildItem -LiteralPath $candidate -File -Recurse|Where-Object Name -Match '^(?:opengl32|libgallium|libglapi|dxil|d3d10warp)\.dll$').Count){throw 'Hosted graphics dependencies must never be packaged'}
}
if(Test-Path $profile){throw 'Existing native profile left untouched'}
New-Item -ItemType Directory -Path $profile|Out-Null
[IO.File]::WriteAllText((Join-Path $profile '.tlauncher-native-trial'),"tlauncher-native isolated profile v1`n",[Text.UTF8Encoding]::new($false))
if($Wallpaper){[IO.File]::WriteAllText((Join-Path $profile 'wallpaper-v1.ini'),"schema=1`nenabled=1`nscene=beach`nlayout=1`nfps=25`nresolution=1`npauseCovered=1`nweather=0`nsolar=1`nclockLighting=1`n",[Text.UTF8Encoding]::new($false))}
$verified=[ordered]@{sourceCommit=$Commit;runId=$RunId;workflow=$run.html_url;artifactId=$art[0].id;artifactSha256=$art[0].digest.Substring(7);archiveSha256=$wanted;version='1.0.3';profile=$profile;productionExe=$ProductionExe;files=$fileManifest;executed=$false;ownerAcceptance='PENDING';publicRelease='NOT_PUBLISHED'}
$verified|ConvertTo-Json -Depth 6|Set-Content (Join-Path $candidate 'verified-native-artifact.json') -Encoding UTF8
$entryPoint=Join-Path $root ('START-'+$(if($Wallpaper){'WALLPAPER'}else{'NATIVE'})+'-TEST-'+$Commit.Substring(0,12)+'.cmd')
$text=@'
@echo off
setlocal EnableExtensions DisableDelayedExpansion
title TLauncher 1.0.3 NATIVE BASE TEST
echo Leave this window open. Exit TEST normally to restore production.
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -STA -ExecutionPolicy Bypass -File "%~dp0__CANDIDATE__\native-owner-uat.ps1"
if errorlevel 1 pause
'@
if($Wallpaper){$text=$text.Replace('NATIVE BASE TEST','WALLPAPER TEST')}
[IO.File]::WriteAllText($entryPoint,($text.Replace('__CANDIDATE__',[IO.Path]::GetFileName($candidate))-replace '\r?\n',"`r`n"),[Text.Encoding]::ASCII)
$linkPath=Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) ('TLauncher 1.0.3 '+[char]0x2014+$(if($Wallpaper){' WALLPAPER TEST.lnk'}else{' NATIVE BASE TEST.lnk'}))
Assert-NoReparse $linkPath
$wsh=New-Object -ComObject WScript.Shell;$link=$wsh.CreateShortcut($linkPath)
$link.TargetPath=Join-Path $env:SystemRoot 'System32\cmd.exe';$link.Arguments='/d /c .\'+[IO.Path]::GetFileName($entryPoint)
$link.WorkingDirectory=$root;$link.IconLocation=$exe+',0';$link.WindowStyle=1
$link.Description=if($Wallpaper){'TLauncher Phase 2 WALLPAPER owner TEST. Exit normally to restore stable 1.0.2.'}else{'TLauncher native Phase 1 owner TEST. Exit normally to restore stable 1.0.2.'};$link.Save()
$readback=$wsh.CreateShortcut($linkPath)
if($readback.TargetPath -ine  $link.TargetPath -or  $readback.Arguments -cne  $link.Arguments -or  $readback.WorkingDirectory -ine  $root){throw 'Shortcut readback mismatch'}
if((Get-FileHash -LiteralPath $ProductionExe).Hash -cne  $stableHash){throw 'Production file changed'}
$verified['desktopShortcut']=$linkPath
$verified|ConvertTo-Json -Depth 6|Set-Content (Join-Path $download 'handoff.json') -Encoding UTF8
[pscustomobject]@{candidate=$candidate;profile=$profile;shortcut=$linkPath;commit=$Commit;packageSha256=$wanted;executed=$false}
