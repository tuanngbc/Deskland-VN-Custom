# Windows PowerShell 5.1, only invoked by the owner's TEST shortcut.
# No installation, elevation, global settings changes, or forced termination.
[CmdletBinding()]
param(
    [string]$CandidateDir,
    [string]$ProfileDir,
    [string]$ProductionExe,
    [string]$ExpectedCommit,
    [string]$ExpectedExeSha256,
    [switch]$RestoreOnly,
    [switch]$LibraryOnly
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Same-Path([string]$A,[string]$B) {
    if (-not $A -or -not $B) { return $false }
    return [IO.Path]::GetFullPath($A).Equals([IO.Path]::GetFullPath($B),[StringComparison]::OrdinalIgnoreCase)
}

function Get-LaunchPlan($Processes,[string]$Production,[string]$Candidate) {
    $main = @(); $guards = @()
    foreach ($p in @($Processes)) {
        if (-not $p.ExecutablePath -or -not $p.CommandLine) { throw 'Cannot verify a TLauncher process identity.' }
        if (Same-Path $p.ExecutablePath $Candidate) { throw 'This candidate is already running. Exit it first.' }
        if (-not (Same-Path $p.ExecutablePath $Production)) { throw 'Another TLauncher path is active. No process has been stopped.' }
        if ($p.CommandLine -match '(?i)(?:\s|^)--dock-taskbar-guard(?:\s|$)') { $guards += $p }
        elseif ($p.CommandLine -match '(?i)(?:\s|^)(--trial-dir|--saver|/s|--quit)(?:\s|$)') { throw 'Unexpected production process role.' }
        else { $main += $p }
    }
    if ($main.Count -gt 1 -or ($main.Count -eq 0 -and $guards.Count -gt 0)) { throw 'Ambiguous production ownership; no process has been stopped.' }
    [pscustomobject]@{ WasRunning=($main.Count -eq 1); Main=($main | Select-Object -First 1); Guards=$guards }
}

function Get-SessionProcesses {
    $session = (Get-Process -Id $PID).SessionId
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    foreach ($p in @(Get-CimInstance Win32_Process -Filter "Name='TLauncher.exe'" | Where-Object SessionId -eq $session)) {
        $owner = Invoke-CimMethod -InputObject $p -MethodName GetOwnerSid
        if ($owner.ReturnValue -ne 0 -or $owner.Sid -ne $sid) { throw 'Cannot verify ownership of a TLauncher process in this session.' }
        $p
    }
}

function Test-Singleton {
    try { $m=[Threading.Mutex]::OpenExisting('TuanpcDesk_Singleton'); $m.Dispose(); return $true }
    catch [Threading.WaitHandleCannotBeOpenedException] { return $false }
}

function Wait-Vacant([int]$Seconds=35) {
    $deadline=[DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if (@(Get-SessionProcesses).Count -eq 0 -and -not (Test-Singleton)) { return $true }
        Start-Sleep -Milliseconds 350
    } while ([DateTime]::UtcNow -lt $deadline)
    return $false
}

function Assert-NoReparse([string]$Path) {
    $p=[IO.Path]::GetFullPath($Path)
    while ($p) {
        if (Test-Path -LiteralPath $p) {
            if ((Get-Item -LiteralPath $p -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'UAT path contains a junction or symbolic link.' }
        }
        $parent=[IO.Path]::GetDirectoryName($p)
        if (-not $parent -or $parent -eq $p) { break }; $p=$parent
    }
}

function Assert-Candidate([string]$Dir,[string]$Commit,[string]$ExeHash) {
    Assert-NoReparse $Dir
    $exe=Join-Path $Dir 'TLauncher.exe'
    foreach ($name in @('TLauncher.exe','BUILD-COMMIT.txt','VERSION.txt','prototype\settings.html','assets\widget')) {
        $item=Join-Path $Dir $name
        Assert-NoReparse $item
        if (-not (Test-Path -LiteralPath $item)) { throw "Candidate file is missing: $name" }
    }
    if ($Commit -notmatch '^[a-f0-9]{40}$' -or $ExeHash -notmatch '^[a-fA-F0-9]{64}$') { throw 'Invalid artifact identity.' }
    if ((Get-Content -LiteralPath (Join-Path $Dir 'BUILD-COMMIT.txt') -Raw).Trim() -cne $Commit -or
        (Get-Content -LiteralPath (Join-Path $Dir 'VERSION.txt') -Raw).Trim() -cne '1.0.2' -or
        (Get-Item -LiteralPath $exe).VersionInfo.ProductVersion -cne '1.0.2' -or
        (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $ExeHash) { throw 'Candidate version, commit, or checksum does not match.' }
    return $exe
}

function Initialize-Profile([string]$Dir,[string]$Root) {
    $parent=Join-Path $Root 'profiles'
    if (-not (Same-Path ([IO.Path]::GetDirectoryName($Dir)) $parent) -or
        [IO.Path]::GetFileName($Dir) -notmatch '^tuanpc-desk-trial-[a-zA-Z0-9-]+$') { throw 'Unsafe isolated profile directory.' }
    Assert-NoReparse $Dir
    $marker=Join-Path $Dir '.tuanpc-desk-trial'
    Assert-NoReparse $marker
    $expected="tuanpc-desk isolated native smoke profile v1`n"
    if (Test-Path -LiteralPath $Dir) {
        if (@(Get-ChildItem -LiteralPath $Dir -Force).Count -gt 0 -and
            (-not (Test-Path -LiteralPath $marker) -or [IO.File]::ReadAllText($marker) -cne $expected)) { throw 'Nonempty UAT profile has no valid marker.' }
    } else { New-Item -ItemType Directory -Path $Dir -Force | Out-Null }
    [IO.File]::WriteAllText($marker,$expected,[Text.UTF8Encoding]::new($false))
    $source=Join-Path $env:USERPROFILE '.deskland-rs'
    $settings=Join-Path $Dir 'settings.json'
    if (-not (Test-Path -LiteralPath $settings) -and (Test-Path -LiteralPath (Join-Path $source 'settings.json'))) {
        Copy-Item -LiteralPath (Join-Path $source 'settings.json') -Destination $settings
        $icons=Join-Path $source 'dock-icons'
        if (Test-Path -LiteralPath $icons) {
            $target=Join-Path $Dir 'dock-icons'; Assert-NoReparse $target; New-Item -ItemType Directory -Path $target -Force | Out-Null
            Get-ChildItem -LiteralPath $icons -File | Where-Object {$_.Name -match '^[a-fA-F0-9]{32}\.png$' -and $_.Length -le 12582912 -and -not ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)} |
                Select-Object -First 128 | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $target $_.Name) }
        }
    }
}

if ($LibraryOnly) { return }
Add-Type -AssemblyName System.Windows.Forms
function Notify([string]$Message) { [void][Windows.Forms.MessageBox]::Show($Message,'TLauncher 1.0.2 TEST') }

$ownerMutex=$null; $locked=$false; $restore=$false; $phase='validation'; $stateFile=$null
try {
    $ownerMutex=[Threading.Mutex]::new($false,'Local\TLauncherOwnerUatSwitch')
    try { $locked=$ownerMutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $locked=$true }
    if (-not $locked) { throw 'A UAT switch is already in progress.' }
    $exe=Join-Path $CandidateDir 'TLauncher.exe'
    if (-not $RestoreOnly) { $exe=Assert-Candidate $CandidateDir $ExpectedCommit $ExpectedExeSha256 }
    $root=[IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($CandidateDir))
    if (-not (Same-Path $root (Join-Path $env:LOCALAPPDATA 'TLauncher-UAT'))) { throw 'Candidate must be in the dedicated UAT directory.' }
    Assert-NoReparse $ProductionExe
    if (-not (Test-Path -LiteralPath $ProductionExe -PathType Leaf) -or (Same-Path $ProductionExe $exe)) { throw 'Production path is missing or points to candidate.' }
    $stateFile=Join-Path $CandidateDir 'uat-state.json'
    Assert-NoReparse $stateFile
    $plan=Get-LaunchPlan @(Get-SessionProcesses) $ProductionExe $exe
    if ($RestoreOnly) {
        if ($plan.WasRunning) { Notify 'Production is already running.'; return }
        if (-not (Test-Path -LiteralPath $stateFile)) { throw 'No saved production recovery state.' }
        $saved=Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json
        if (-not $saved.wasRunning -or -not (Same-Path $saved.production $ProductionExe)) { throw 'No verified production session to restore.' }
        $restore=$true
    } else {
        $restore=$plan.WasRunning
        @{wasRunning=$restore;production=$ProductionExe;workingDirectory=[IO.Path]::GetDirectoryName($ProductionExe);commit=$ExpectedCommit;phase='switching'} | ConvertTo-Json | Set-Content -LiteralPath $stateFile -Encoding UTF8
        $phase='stopping production'
        if ($restore) {
            # Verify the same PID, creation timestamp and executable path before
            # invoking the existing singleton quit command. Never kill by name.
            $now=Get-LaunchPlan @(Get-SessionProcesses) $ProductionExe $exe
            if (-not $now.WasRunning -or $now.Main.ProcessId -ne $plan.Main.ProcessId -or $now.Main.CreationDate -ne $plan.Main.CreationDate) { throw 'Production identity changed; aborting switch.' }
            Start-Process -FilePath $ProductionExe -ArgumentList '--quit' -WorkingDirectory ([IO.Path]::GetDirectoryName($ProductionExe)) -WindowStyle Hidden -Wait
        }
        if (-not (Wait-Vacant)) { throw 'Production did not exit safely or the singleton is busy. No process was forcibly terminated.' }
        $phase='isolated profile'
        Initialize-Profile $ProfileDir $root
        $phase='candidate launch'
        $process=Start-Process -FilePath $exe -ArgumentList ('--trial-dir "'+$ProfileDir+'"') -WorkingDirectory $CandidateDir -PassThru
        $started=[DateTime]::UtcNow
        Add-Content -LiteralPath (Join-Path $CandidateDir 'uat-session.log') -Value ("{0:o} candidate PID={1} commit={2}" -f $started,$process.Id,$ExpectedCommit)
        $process.WaitForExit()
        Add-Content -LiteralPath (Join-Path $CandidateDir 'uat-session.log') -Value ("{0:o} candidate exit={1}" -f [DateTime]::UtcNow,$process.ExitCode)
        if ($process.ExitCode -ne 0 -or ([DateTime]::UtcNow-$started).TotalSeconds -lt 2) { Notify 'The candidate stopped unexpectedly. Attempting to restore production.' }
    }
} catch { Notify ("UAT stopped at $phase.`n"+$_.Exception.Message) }
finally {
    if ($locked -and $restore) {
        try {
            $running=@(Get-SessionProcesses)
            if ($running.Count -eq 0 -or (@($running | Where-Object {-not (Same-Path $_.ExecutablePath $ProductionExe)}).Count -gt 0)) {
                if (-not (Wait-Vacant)) { throw 'A launcher still owns the desktop. Exit it normally before restoring production.' }
                Start-Process -FilePath $ProductionExe -WorkingDirectory ([IO.Path]::GetDirectoryName($ProductionExe))
            } else {
                $plan=Get-LaunchPlan $running $ProductionExe (Join-Path $CandidateDir 'TLauncher.exe')
                if (-not $plan.WasRunning) { throw 'Production recovery is waiting for its previous guard to exit.' }
            }
        } catch { Notify ("Production recovery requires attention. Use RESTORE-PRODUCTION.cmd in the candidate folder after the test exits.`n"+$_.Exception.Message) }
    }
    if ($locked -and $ownerMutex) { $ownerMutex.ReleaseMutex() }
    if ($ownerMutex) { $ownerMutex.Dispose() }
}
