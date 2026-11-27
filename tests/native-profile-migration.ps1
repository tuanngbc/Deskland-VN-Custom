# Hosted-only integration of the real file + registry migration implementation.
param([string]$NativeContract='', [string]$Evidence='')
if($env:GITHUB_ACTIONS -ne 'true'){throw 'Hosted verification only.'}
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
Add-Type -AssemblyName System.Windows.Forms
$monitorSettings=([Windows.Forms.Screen]::AllScreens|ForEach-Object {"monitor.$($_.DeviceName).enabled=1`nmonitor.$($_.DeviceName).scene=aquarium`nmonitor.$($_.DeviceName).param.aquarium.fish=6`n"}) -join ''
[IO.File]::WriteAllText((Join-Path $source 'wallpaper-v1.ini'),"schema=1`nenabled=1`nscene=lake`nfps=29`nlayout=2`nresolution=0.75`npauseCovered=0`nweather=5`nclockLighting=0`nhour=17.25`nwind=1.4`nparam.lake.depth=321`nunknown.owner=keep-me`n$monitorSettings")
[IO.File]::WriteAllText((Join-Path $source 'assets\custom.txt'),'Tài nguyên của chủ máy')
[IO.File]::WriteAllText((Join-Path $source 'guardian.journal'),'must not migrate')
@{profile=$source}|ConvertTo-Json|Set-Content (Join-Path $candidate 'verified-native-artifact.json') -Encoding UTF8
[IO.File]::WriteAllText((Join-Path $candidate 'native-uat-session.log'),'owner launch')
$snapshot=@(
 @{key='Impostazioni';name='Lingua';kind='DWord';value=7},
 @{key='Impostazioni';name='DockVisibility';kind='DWord';value=2},
 @{key='Impostazioni';name='NotchEnabled';kind='DWord';value=0},
 @{key='Impostazioni';name='StageEnabled';kind='DWord';value=1},
 @{key='Impostazioni';name='StageHeight';kind='DWord';value=174},
 @{key='Impostazioni';name='StagePeekDelay';kind='DWord';value=750},
 @{key='Impostazioni';name='SpotlightMode';kind='DWord';value=2},
 @{key='Impostazioni';name='SpotlightHotkeyMods';kind='DWord';value=3},
 @{key='Impostazioni';name='SpotlightHotkeyKey';kind='DWord';value=133},
 @{key='Impostazioni';name='StageHotkeyMods';kind='DWord';value=5},
 @{key='Impostazioni';name='StageHotkeyKey';kind='DWord';value=83},
 @{key='Impostazioni';name='StageHotkeyEnabled';kind='DWord';value=0},
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
if($NativeContract){
    $beforeProfile=$env:TLAUNCHER_NATIVE_TRIAL_DIR
    try{
        $env:TLAUNCHER_NATIVE_TRIAL_DIR=$destination
        & $NativeContract $Evidence '--verify-migrated'
        if($LASTEXITCODE -ne 0){throw "Migrated native profile verification failed: $LASTEXITCODE"}
        Copy-Item (Join-Path $Evidence 'native-contract.txt') (Join-Path $Evidence 'native-migrated-contract.txt')
        @{profile=$destination;registry=(Get-NativeRegistryPath $destination);commit=$env:GITHUB_SHA}|ConvertTo-Json|Set-Content (Join-Path $Evidence 'profile-migration-runtime.json') -Encoding UTF8
    }finally{$env:TLAUNCHER_NATIVE_TRIAL_DIR=$beforeProfile}
}
$rejected=$false
try{$null=Initialize-NativePersistentProfile $root $destination ('4'*40) {throw 'Shell is still active'}}catch{$rejected=$true}
if(!$rejected){throw 'Live source migration was allowed.'}
"CUSTOM_FILES_REGISTRY=PASS`nUNKNOWN_FIELDS=PASS`nRECOVERY_MARKERS_EXCLUDED=PASS`nIDEMPOTENT=PASS`nROLLBACK=PASS`nFAILED_COMMIT_RECOVERY=PASS`nLIVE_SESSION_BLOCK=PASS`nPRODUCTION_FIXTURE_UNCHANGED=PASS"
