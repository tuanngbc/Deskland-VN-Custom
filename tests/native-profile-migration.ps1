# Hosted-only integration of the real file + registry migration implementation.
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot '../tools/legacy-uat-library.ps1') -LibraryOnly
. (Join-Path $PSScriptRoot '../tools/native-profile-library.ps1')
$root=Join-Path $env:RUNNER_TEMP ('profile-contract-'+[Guid]::NewGuid().ToString('N')+'\TLauncher-UAT')
$source=Join-Path $root 'profiles\tlauncher-native-trial-wallpaper-customized'
$destination=Join-Path $root 'profiles\tlauncher-native-trial-owner-1-0-3'
$candidate=Join-Path $root 'wallpaper-old'
[IO.Directory]::CreateDirectory((Join-Path $source 'assets'))|Out-Null
[IO.Directory]::CreateDirectory($candidate)|Out-Null
[IO.File]::WriteAllText((Join-Path $source '.tlauncher-native-trial'),"tlauncher-native isolated profile v1`n")
[IO.File]::WriteAllText((Join-Path $source 'wallpaper-v1.ini'),"schema=1`nenabled=1`nscene=lake`nfps=29`nlayout=2`nunknown.owner=keep-me`nmonitor.test=portrait-negative-1.5`n")
[IO.File]::WriteAllText((Join-Path $source 'assets\custom.txt'),'Tài nguyên của chủ máy')
[IO.File]::WriteAllText((Join-Path $source 'guardian.journal'),'must not migrate')
@{profile=$source}|ConvertTo-Json|Set-Content (Join-Path $candidate 'verified-native-artifact.json') -Encoding UTF8
[IO.File]::WriteAllText((Join-Path $candidate 'native-uat-session.log'),'owner launch')
$snapshot=@(
 @{key='Impostazioni';name='Lingua';kind='DWord';value=7},
 @{key='Impostazioni';name='DockVisibility';kind='DWord';value=2},
 @{key='Impostazioni';name='NotchEnabled';kind='DWord';value=0},
 @{key='Impostazioni';name='StageHeight';kind='DWord';value=174},
 @{key='Impostazioni';name='StagePeekDelay';kind='DWord';value=750},
 @{key='Impostazioni';name='SpotlightMode';kind='DWord';value=2},
 @{key='';name='OrdineFissate';kind='MultiString';value=@('app-b','app-a')},
 @{key='';name='StatoBarra';kind='DWord';value=1},
 @{key='';name='UnknownBinary';kind='Binary';value='AQIDBA=='},
 @{key='Catalog\Apps\custom';name='Name';kind='String';value='Công việc'},
 @{key='Catalog\Apps\custom';name='Icon';kind='String';value=(Join-Path $source 'assets\custom.txt')}
)|ForEach-Object {[pscustomobject]$_}
Set-NativeRegistrySnapshot $source $snapshot
$sourceHash=(Get-FileHash (Join-Path $source 'wallpaper-v1.ini')).Hash
$production=Join-Path $root 'production-fixture.json';[IO.File]::WriteAllText($production,'production-profile-must-stay-unchanged');$productionHash=(Get-FileHash $production).Hash
$commit='1111111111111111111111111111111111111111'
$first=Initialize-NativePersistentProfile $root $destination $commit {}
if((Get-FileHash (Join-Path $destination 'wallpaper-v1.ini')).Hash -cne $sourceHash){throw 'Wallpaper settings changed.'}
if(Test-Path (Join-Path $destination 'guardian.journal')){throw 'Recovery journal copied.'}
$actual=@(Get-NativeRegistrySnapshot $destination)
foreach($expected in $snapshot|Where-Object name -ne 'StatoBarra') {
 $entry=@($actual|Where-Object {$_.key -ceq $expected.key -and $_.name -ceq $expected.name})
 if($entry.Count -ne 1 -or $entry[0].kind -cne $expected.kind){throw "Registry field missing: $($expected.name)"}
 $want=if($expected.kind -eq 'String'){([string]$expected.value).Replace($source,$destination)}else{$expected.value}
 if((ConvertTo-Json -InputObject $entry[0].value -Compress) -cne (ConvertTo-Json -InputObject $want -Compress)){throw "Registry value changed: $($expected.name)"}
}
if(@($actual|Where-Object name -eq 'StatoBarra').Count){throw 'Recovery registry marker copied.'}
$second=Initialize-NativePersistentProfile $root $destination ('2'*40) {}
if((Get-FileHash (Join-Path $destination 'wallpaper-v1.ini')).Hash -cne $sourceHash){throw 'Repeated migration reset settings.'}
$rollback=Restore-NativeCompatibleSnapshot $root $second.snapshot $commit 1 {}
if((Get-FileHash (Join-Path $rollback 'wallpaper-v1.ini')).Hash -cne $sourceHash){throw 'Rollback lost settings.'}
try {$null=Initialize-NativePersistentProfile $root $destination ('3'*40) {} {throw 'Simulated registry write failure'};throw 'Expected migration failure'}catch{if($_.Exception.Message -ne 'Simulated registry write failure'){throw}}
if((Get-FileHash (Join-Path $destination 'wallpaper-v1.ini')).Hash -cne $sourceHash -or (Get-FileHash (Join-Path $source 'wallpaper-v1.ini')).Hash -cne $sourceHash -or (Get-FileHash $production).Hash -cne $productionHash){throw 'Failure recovery or production preservation failed.'}
$rejected=$false
try{$null=Initialize-NativePersistentProfile $root $destination ('4'*40) {throw 'Shell is still active'}}catch{$rejected=$true}
if(!$rejected){throw 'Live source migration was allowed.'}
"CUSTOM_FILES_REGISTRY=PASS`nUNKNOWN_FIELDS=PASS`nRECOVERY_MARKERS_EXCLUDED=PASS`nIDEMPOTENT=PASS`nROLLBACK=PASS`nFAILED_COMMIT_RECOVERY=PASS`nLIVE_SESSION_BLOCK=PASS`nPRODUCTION_FIXTURE_UNCHANGED=PASS"
