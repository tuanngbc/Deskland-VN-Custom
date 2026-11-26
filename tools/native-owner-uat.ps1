# Invoked only by owner's Native Base TEST shortcut. No forced termination.
[CmdletBinding()]
param([switch]$RestoreOnly)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$requestedRestore=$RestoreOnly
. (Join-Path $PSScriptRoot 'legacy-uat-library.ps1') -LibraryOnly
$RestoreOnly=$requestedRestore
$mutex=[Threading.Mutex]::new($false,'Local\TLauncherOwnerUatSwitch')
$held=$false;$restore=$false;$failure=$null;$resumeAutostart=$false
$statePath=Join-Path $PSScriptRoot 'native-uat-state.json'
function Native-MutexExists {
    try{$m=[Threading.Mutex]::OpenExisting('Local\TLauncher.Native.Istanza');$m.Dispose();return $true}
    catch [Threading.WaitHandleCannotBeOpenedException]{return $false}
}
function Wait-NativeVacant([int]$Seconds=60) {
    $until=[DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if(@(Get-SessionProcesses).Count -eq 0 -and !(Native-MutexExists) -and !(Test-Singleton)){return $true}
        Start-Sleep -Milliseconds 300
    } while([DateTime]::UtcNow -lt $until)
    return $false
}
try {
    try{$held=$mutex.WaitOne(0)}catch [Threading.AbandonedMutexException]{$held=$true}
    if(!$held){throw 'Another TEST session is active. Exit that TEST normally and wait for production to return.'}
    $manifestPath=Join-Path $PSScriptRoot 'verified-native-artifact.json'
    Assert-NoReparse $manifestPath
    $verified=Get-Content -LiteralPath $manifestPath -Raw|ConvertFrom-Json
    $production=$verified.productionExe
    $candidate=Join-Path $PSScriptRoot 'TLauncher.exe'
    $profile=$verified.profile
    foreach($path in @($candidate,$production,$profile,$statePath)){Assert-NoReparse $path}
    if((Get-Item -LiteralPath $production).VersionInfo.ProductVersion -cne '1.0.2'){throw 'Expected stable production 1.0.2.'}
    if((Get-Content (Join-Path $PSScriptRoot 'BUILD-COMMIT.txt') -Raw).Trim() -cne $verified.sourceCommit -or
       (Get-Content (Join-Path $PSScriptRoot 'UPSTREAM-COMMIT.txt') -Raw).Trim() -cne '2f773a8733b84c7dbee88fff6628d14e9ac802b9' -or
       (Get-Content (Join-Path $PSScriptRoot 'VERSION.txt') -Raw).Trim() -cne '1.0.3' -or
       (Get-Item $candidate).VersionInfo.ProductVersion -cne '1.0.3') {throw 'Candidate metadata mismatch.'}
    foreach($file in $verified.files){
        $path=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot $file.path))
        if(!$path.StartsWith($PSScriptRoot+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Uncontained manifest path'}
        Assert-NoReparse $path
        if((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $file.sha256){throw "Candidate changed: $($file.path)"}
    }
    $trialRoot=Split-Path -Parent (Split-Path -Parent $profile)
    if([IO.Path]::GetFileName($trialRoot) -ine 'TLauncher-UAT' -or
       [IO.Path]::GetFileName((Split-Path -Parent $profile)) -ine 'profiles' -or
       [IO.Path]::GetFileName($profile) -notmatch '^tlauncher-native-trial-[a-zA-Z0-9-]+$' -or
       [IO.File]::ReadAllText((Join-Path $profile '.tlauncher-native-trial')) -cne "tlauncher-native isolated profile v1`n"){throw 'Invalid native TEST profile'}
    # Do not run alongside original FluidWin or any other TLauncher candidate.
    $session=(Get-Process -Id $PID).SessionId
    if(@(Get-CimInstance Win32_Process -Filter "Name='FluidWin.exe'"|Where-Object SessionId -eq $session).Count){throw 'Exit original FluidWin normally before starting TEST.'}
    $plan=Get-LaunchPlan @(Get-SessionProcesses) $production $candidate
    if($RestoreOnly){
        if($plan.WasRunning){return}
        $saved=Get-Content -LiteralPath $statePath -Raw|ConvertFrom-Json
        if(!$saved.wasRunning -or !(Same-Path $saved.production $production)){throw 'No production recovery state'}
        $resumeAutostart=$saved.autostartRole
        $restore=$true
    }else{
        $resumeAutostart=$plan.WasRunning -and $plan.Main.CommandLine -match '(?:^|\s)--autostart(?:\s|$)'
        @{wasRunning=$plan.WasRunning;production=$production;sourceCommit=$verified.sourceCommit;profile=$profile;autostartRole=$resumeAutostart}|
            ConvertTo-Json|Set-Content -LiteralPath $statePath -Encoding UTF8
        if($plan.WasRunning){
            $now=Get-LaunchPlan @(Get-SessionProcesses) $production $candidate
            if(!$now.WasRunning -or $now.Main.ProcessId -ne $plan.Main.ProcessId -or $now.Main.CreationDate -ne $plan.Main.CreationDate){throw 'Production identity changed; nothing stopped.'}
            $restore=$true
            Start-Process -FilePath $production -ArgumentList '--quit' -WorkingDirectory (Split-Path -Parent $production) -WindowStyle Hidden -Wait
        }
        if(!(Wait-NativeVacant 45)){throw 'Previous shell/Guardian has not exited safely. Nothing was force-killed.'}
        $env:TLAUNCHER_NATIVE_TRIAL_DIR=$profile
        $p=Start-Process -FilePath $candidate -ArgumentList @('--trial-dir',('"'+$profile+'"'),'--log') -WorkingDirectory $PSScriptRoot -PassThru
        Add-Content -LiteralPath (Join-Path $PSScriptRoot 'native-uat-session.log') -Value ("{0:o} owner launch PID={1} commit={2}" -f [DateTime]::UtcNow,$p.Id,$verified.sourceCommit)
        $ready=$false;$until=[DateTime]::UtcNow.AddSeconds(20)
        do {
            $guards=@(Get-SessionProcesses | Where-Object { (Same-Path $_.ExecutablePath $candidate) -and $_.CommandLine -match ('\s--guardiano\s+'+$p.Id+'\s') })
            if($guards.Count -eq 1){$ready=$true;break}
            if($p.HasExited){break};Start-Sleep -Milliseconds 250
        }while([DateTime]::UtcNow -lt $until)
        if(!$ready){
            Start-Process -FilePath $candidate -ArgumentList '--quit' -WorkingDirectory $PSScriptRoot -WindowStyle Hidden -Wait
            throw 'Native Guardian did not become ready. TEST was asked to exit normally.'
        }
        $p.WaitForExit()
        # Original FluidWin can restart for DPI/GPU changes. Keep waiting for the
        # exact native session and its detached Guardian before restoring stable.
        while(@(Get-SessionProcesses|Where-Object {Same-Path $_.ExecutablePath $candidate}).Count -or (Native-MutexExists)) {
            Start-Sleep -Milliseconds 500
        }
        if($p.ExitCode -ne 0){throw "Native TEST exited with code $($p.ExitCode)."}
    }
}catch{$failure=$_.Exception.Message;Write-Warning $failure}
finally{
    if($held -and $restore){
        try{
            $remaining=@(Get-SessionProcesses)
            if(!$remaining.Count){
                if(!(Wait-NativeVacant 45)){throw 'Recovery is still active; production was not restarted concurrently.'}
                if($resumeAutostart){Start-Process -FilePath $production -ArgumentList '--autostart' -WorkingDirectory (Split-Path -Parent $production) -WindowStyle Hidden}
                else{Start-Process -FilePath $production -WorkingDirectory (Split-Path -Parent $production) -WindowStyle Hidden}
            }else{
                $plan=Get-LaunchPlan $remaining $production $candidate
                if(!$plan.WasRunning){throw 'Wait for Guardian before reopening production.'}
            }
        }catch{$failure=(@($failure,$_.Exception.Message)|Where-Object {$_})-join "`n";Write-Warning $_}
    }
    if($held){$mutex.ReleaseMutex()};$mutex.Dispose()
}
if($failure){Write-Error $failure;exit 1}
