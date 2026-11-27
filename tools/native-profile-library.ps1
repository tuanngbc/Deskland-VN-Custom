# Source only. Invoked after all desktop shells/Guardians exit, on owner launch.
# Production and upstream FluidWin namespaces are never valid inputs.
Set-StrictMode -Version Latest
function Assert-NativeProfilePath([string]$Path,[string]$Root) {
    $full=[IO.Path]::GetFullPath($Path).TrimEnd('\')
    $profiles=[IO.Path]::GetFullPath((Join-Path $Root 'profiles')).TrimEnd('\')
    if((Split-Path -Parent $full) -ine $profiles -or (Split-Path -Leaf $full) -notmatch '^tlauncher-native-trial-[a-zA-Z0-9-]+$'){throw 'Profile is outside isolated native UAT namespace.'}
    Assert-NoReparse $full
    return $full
}
function Get-NativeRegistryPath([string]$Profile) {
    $leaf=Split-Path -Leaf $Profile
    if($leaf -notmatch '^tlauncher-native-trial-[a-zA-Z0-9-]+$'){throw 'Invalid registry profile identity.'}
    return 'Software\TLauncher\NativeBase\Profiles\'+$leaf
}
function Get-NativeRegistrySnapshot([string]$Profile) {
    $base=Get-NativeRegistryPath $Profile
    $out=[Collections.Generic.List[object]]::new()
    function Read-NativeKey([string]$Relative) {
        $path=$base;if($Relative){$path+='\'+$Relative}
        $key=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($path,$false)
        if(!$key){return}
        try {
            foreach($name in $key.GetValueNames()) {
                # Audited recovery markers; retain every other value, including unknown future settings.
                if(!$Relative -and $name -in @('StatoBarra','SnapDaRimettere','CursoriAttivi')){continue}
                if($Relative -match '^(?:Runtime|Recovery|Guardian|Capture)(?:\\|$)'){continue}
                $kind=$key.GetValueKind($name).ToString()
                $value=$key.GetValue($name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                if($kind -eq 'Binary' -or $kind -eq 'None'){$value=[Convert]::ToBase64String([byte[]]$value)}
                if($kind -eq 'QWord'){$value=$value.ToString([Globalization.CultureInfo]::InvariantCulture)}
                $out.Add([pscustomobject]@{key=$Relative;name=$name;kind=$kind;value=$value})
            }
            foreach($child in $key.GetSubKeyNames()){Read-NativeKey $(if($Relative){$Relative+'\'+$child}else{$child})}
        }finally{$key.Dispose()}
    }
    Read-NativeKey ''
    return $out.ToArray()
}
function Set-NativeRegistrySnapshot([string]$Profile,[object[]]$Snapshot,[string]$SourceProfile='') {
    $base=Get-NativeRegistryPath $Profile
    foreach($entry in $Snapshot) {
        if($entry.key -match '(?:^|\\)\.\.?(?:\\|$)' -or $entry.key.StartsWith('\')){throw 'Invalid snapshot registry path.'}
        $path=$base;if($entry.key){$path+='\'+$entry.key}
        $key=[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($path)
        try {
            $kind=[Microsoft.Win32.RegistryValueKind][Enum]::Parse([Microsoft.Win32.RegistryValueKind],$entry.kind)
            $value=$entry.value
            switch($entry.kind) {
                'Binary' {$value=[Convert]::FromBase64String([string]$value)}
                'None' {$value=[Convert]::FromBase64String([string]$value)}
                'DWord' {$value=[int]$value}
                'QWord' {$value=[long]::Parse([string]$value,[Globalization.CultureInfo]::InvariantCulture)}
                'MultiString' {$value=[string[]]@($value)}
            }
            if($SourceProfile) {
                if($entry.kind -in @('String','ExpandString')){$value=([string]$value).Replace($SourceProfile,$Profile)}
                if($entry.kind -eq 'MultiString'){$value=[string[]]@($value|ForEach-Object {$_.Replace($SourceProfile,$Profile)})}
            }
            $key.SetValue($entry.name,$value,$kind)
        }finally{$key.Dispose()}
    }
}
function Get-NativePersistentFiles([string]$Profile) {
    # Reject reparse points before recursing. Never follow links into owner data.
    $pending=[Collections.Generic.Queue[string]]::new();$pending.Enqueue($Profile)
    while($pending.Count) {
        foreach($item in Get-ChildItem -LiteralPath $pending.Dequeue() -Force) {
            if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Profile contains a junction or symbolic link.'}
            if($item.Name -match '^(?:runtime|recovery|guardian|capture)(?:[.-]|$)' -or $item.Name -match '(?:\.log|\.pid|\.tmp|\.journal|\.lock)$' -or $item.Name -eq 'native-profile.json'){continue}
            if($item.PSIsContainer){$pending.Enqueue($item.FullName)}else{$item}
        }
    }
}
function Copy-NativeProfileData([string]$Source,[string]$Destination) {
    [IO.Directory]::CreateDirectory($Destination)|Out-Null
    foreach($file in @(Get-NativePersistentFiles $Source)) {
        $relative=$file.FullName.Substring($Source.TrimEnd('\').Length+1)
        $to=Join-Path $Destination $relative
        Assert-NoReparse $to
        [IO.Directory]::CreateDirectory((Split-Path -Parent $to))|Out-Null
        [IO.File]::Copy($file.FullName,$to,$false)
    }
}
function Get-NativeOwnerSource([string]$Root,[string]$Persistent) {
    # Called only after vacancy. Session logs prove actual owner use; defaults from
    # a newly downloaded artifact never win simply because they are newer.
    $choices=@()
    foreach($dir in Get-ChildItem -LiteralPath $Root -Directory) {
        $manifest=Join-Path $dir.FullName 'verified-native-artifact.json'
        $session=Join-Path $dir.FullName 'native-uat-session.log'
        if(!(Test-Path -LiteralPath $manifest) -or !(Test-Path -LiteralPath $session)){continue}
        Assert-NoReparse $manifest;Assert-NoReparse $session
        $m=Get-Content -LiteralPath $manifest -Raw|ConvertFrom-Json
        $p=Assert-NativeProfilePath $m.profile $Root
        if(!(Test-Path -LiteralPath $p)){continue}
        $when=(Get-Item -LiteralPath $session).LastWriteTimeUtc
        $custom=@(Get-NativeRegistrySnapshot $p).Count+@(Get-NativePersistentFiles $p|Where-Object Name -ne '.tlauncher-native-trial').Count
        $choices+=[pscustomobject]@{path=$p;used=$when;custom=$custom}
    }
    if(Test-Path -LiteralPath $Persistent) {
        $meta=Join-Path $Persistent 'native-profile.json'
        if(Test-Path -LiteralPath $meta){$m=Get-Content -LiteralPath $meta -Raw|ConvertFrom-Json;$choices+=[pscustomobject]@{path=$Persistent;used=[DateTime]::Parse($m.ownerUsedUtc).ToUniversalTime();custom=100000}}
    }
    # An actually used customized profile outranks an empty/default session. Among
    # customized sessions, recency wins, including a subsequently used baseline.
    $chosen=$choices|Sort-Object @{Expression={if($_.custom -gt 2){1}else{0}};Descending=$true},@{Expression='used';Descending=$true}|Select-Object -First 1
    if($chosen){return $chosen.path}
    return $null
}
function Assert-NativeProfileData([string]$Profile) {
    if([IO.File]::ReadAllText((Join-Path $Profile '.tlauncher-native-trial')) -cne "tlauncher-native isolated profile v1`n"){throw 'Invalid isolated profile marker.'}
    $wallpaper=Join-Path $Profile 'wallpaper-v1.ini'
    if(Test-Path -LiteralPath $wallpaper) {
        $text=[IO.File]::ReadAllText($wallpaper)
        if($text -match '(?m)^schema=(\d+)' -and [int]$Matches[1] -gt 1){throw 'Wallpaper schema is newer than this candidate supports.'}
    }
    $null=@(Get-NativePersistentFiles $Profile)
}
function Initialize-NativePersistentProfile([string]$Root,[string]$Destination,[string]$Commit,[scriptblock]$AssertVacant,[scriptblock]$BeforeRegistryCommit={}) {
    & $AssertVacant
    $Destination=Assert-NativeProfilePath $Destination $Root
    $source=Get-NativeOwnerSource $Root $Destination
    $schema=1
    if($source) {
        Assert-NativeProfileData $source
        $meta=Join-Path $source 'native-profile.json'
        if(Test-Path -LiteralPath $meta){$old=Get-Content -LiteralPath $meta -Raw|ConvertFrom-Json;if($old.schema -gt $schema){throw 'Newer schema requires its compatible rollback snapshot.'}}
    }
    $id=[Guid]::NewGuid().ToString('N')
    $backupRoot=Join-Path $Root ('profile-snapshots\'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffffff')+'-'+$Commit.Substring(0,12)+'-'+$id)
    [IO.Directory]::CreateDirectory($backupRoot)|Out-Null
    $stage=Join-Path (Split-Path -Parent $Destination) ('tlauncher-native-trial-migration-'+$id)
    $previous=Join-Path (Split-Path -Parent $Destination) ('tlauncher-native-trial-previous-'+$id)
    $original=@(Get-NativeRegistrySnapshot $Destination)
    $registry=@();$renamed=$false;$installed=$false
    try {
        if($source) {
            $registry=@(Get-NativeRegistrySnapshot $source)
            Copy-NativeProfileData $source (Join-Path $backupRoot 'source')
            if(Test-Path -LiteralPath $meta){[IO.File]::Copy($meta,(Join-Path $backupRoot 'source\native-profile.json'),$false)}
            ConvertTo-Json -InputObject @($registry) -Depth 30|Set-Content -LiteralPath (Join-Path $backupRoot 'registry.json') -Encoding UTF8
            Copy-NativeProfileData $source $stage
        } else {
            [IO.Directory]::CreateDirectory($stage)|Out-Null
            [IO.File]::WriteAllText((Join-Path $stage '.tlauncher-native-trial'),"tlauncher-native isolated profile v1`n",[Text.UTF8Encoding]::new($false))
        }
        # Keep externally stored legacy pins in the isolated profile from now on.
        $pins=Join-Path $stage 'DockPins'
        if(!(Test-Path -LiteralPath $pins)) {
            [IO.Directory]::CreateDirectory($pins)|Out-Null
            $oldPins=Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar'
            if(Test-Path -LiteralPath $oldPins){Assert-NoReparse $oldPins;foreach($f in Get-ChildItem -LiteralPath $oldPins -Filter '*.lnk' -File){Assert-NoReparse $f.FullName;[IO.File]::Copy($f.FullName,(Join-Path $pins $f.Name),$false)}}
        }
        # Migration adds metadata only. Existing values and unknown compatible fields survive.
        $stamp=[ordered]@{schema=$schema;line='native-1.0.3';commit=$Commit;source=$source;ownerUsedUtc=[DateTime]::UtcNow.ToString('o');snapshot=$backupRoot}
        $stamp|ConvertTo-Json -Depth 10|Set-Content -LiteralPath (Join-Path $stage 'native-profile.json') -Encoding UTF8
        Assert-NativeProfileData $stage
        & $AssertVacant
        if(Test-Path -LiteralPath $Destination){Assert-NoReparse $previous;[IO.Directory]::Move($Destination,$previous);$renamed=$true}
        [IO.Directory]::Move($stage,$Destination);$installed=$true
        & $BeforeRegistryCommit
        # Replace only this isolated destination tree; rollback restores its entire prior tree.
        [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree((Get-NativeRegistryPath $Destination),$false)
        Set-NativeRegistrySnapshot $Destination $registry $source
        Assert-NativeProfileData $Destination
        @{source=$source;destination=$Destination;commit=$Commit;schema=$schema;previousDirectory=$(if($renamed){$previous}else{$null});registryBefore=$original;completedUtc=[DateTime]::UtcNow.ToString('o')}|ConvertTo-Json -Depth 30|Set-Content -LiteralPath (Join-Path $backupRoot 'migration.json') -Encoding UTF8
        return [pscustomobject]@{profile=$Destination;source=$source;snapshot=$backupRoot;schema=$schema}
    } catch {
        $failure=$_
        if($installed) {
            # Keep failed data for diagnosis; do not recursively delete owner files.
            $failed=Join-Path (Split-Path -Parent $Destination) ('tlauncher-native-trial-failed-'+$id)
            [IO.Directory]::Move($Destination,$failed)
        }
        if($renamed){[IO.Directory]::Move($previous,$Destination)}
        if($installed){[Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree((Get-NativeRegistryPath $Destination),$false);Set-NativeRegistrySnapshot $Destination $original}
        throw $failure
    }
}
function Restore-NativeCompatibleSnapshot([string]$Root,[string]$Snapshot,[string]$Commit,[int]$SupportedSchema,[scriptblock]$AssertVacant) {
    & $AssertVacant
    $base=[IO.Path]::GetFullPath((Join-Path $Root 'profile-snapshots')).TrimEnd('\')+'\'
    $snapshotPath=[IO.Path]::GetFullPath($Snapshot).TrimEnd('\')
    if(!$snapshotPath.StartsWith($base,[StringComparison]::OrdinalIgnoreCase)){throw 'Snapshot is outside the UAT backup root.'}
    Assert-NoReparse $snapshotPath
    $source=Join-Path $snapshotPath 'source';Assert-NativeProfileData $source
    $meta=Join-Path $source 'native-profile.json'
    if(Test-Path -LiteralPath $meta){$data=Get-Content -LiteralPath $meta -Raw|ConvertFrom-Json;if($data.schema -gt $SupportedSchema -or $data.commit -cne $Commit){throw 'Snapshot is not compatible with the requested candidate.'}}
    $manifest=Get-Content -LiteralPath (Join-Path $snapshotPath 'migration.json') -Raw|ConvertFrom-Json
    $target=Join-Path $Root ('profiles\tlauncher-native-trial-rollback-'+$Commit.Substring(0,12)+'-'+[Guid]::NewGuid().ToString('N'))
    $target=Assert-NativeProfilePath $target $Root
    Copy-NativeProfileData $source $target
    $registry=@(Get-Content -LiteralPath (Join-Path $snapshotPath 'registry.json') -Raw|ConvertFrom-Json)
    Set-NativeRegistrySnapshot $target $registry $manifest.source
    Assert-NativeProfileData $target
    return $target
}
