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
    [switch]$UseVerifiedManifest,
    [switch]$PreflightOnly,
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
    # A process can disappear between enumeration and GetOwnerSid.
    # Discard the whole snapshot if any owner could not be resolved.
    for ($attempt=1; $attempt -le 5; $attempt++) {
        $verified=@()
        $retry=$null
        foreach ($p in @(Get-CimInstance Win32_Process -Filter "Name='TLauncher.exe'" | Where-Object SessionId -eq $session)) {
            try {
                $owner=Invoke-CimMethod -InputObject $p -MethodName GetOwnerSid
                if ($null -eq $owner) { $retry="PID $($p.ProcessId): owner unavailable"; break }
                if ($owner.ReturnValue -ne 0) { $retry="PID $($p.ProcessId): GetOwnerSid returned $($owner.ReturnValue)"; break }
                if (-not $owner.Sid) { $retry="PID $($p.ProcessId): empty owner SID"; break }
            } catch {
                $retry="PID $($p.ProcessId): GetOwnerSid failed: $($_.Exception.Message)"
                break
            }
            if ($owner.Sid -ne $sid) {
                throw "TLauncher PID $($p.ProcessId) is owned by a different account. No process has been stopped."
            }
            $verified += $p
        }
        if (-not $retry) { return $verified }
        if ($attempt -lt 5) { Start-Sleep -Milliseconds 250 }
    }
    throw "Cannot verify TLauncher ownership after retrying. $retry"
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
        (Get-Content -LiteralPath (Join-Path $Dir 'VERSION.txt') -Raw).Trim() -cne '1.0.3' -or
        (Get-Item -LiteralPath $exe).VersionInfo.ProductVersion -cne '1.0.3' -or
        (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $ExeHash) { throw 'Candidate version, commit, or checksum does not match.' }
    return $exe
}

function Test-ProfileLocation([string]$Dir,[string]$Root) {
    $parent=Join-Path $Root 'profiles'
    if (-not (Same-Path ([IO.Path]::GetDirectoryName($Dir)) $parent) -or
        [IO.Path]::GetFileName($Dir) -notmatch '^tuanpc-desk-trial-[a-zA-Z0-9-]+$') { throw 'Unsafe isolated profile directory.' }
    Assert-NoReparse $Dir
}

function Initialize-Profile([string]$Dir,[string]$Root) {
    Test-ProfileLocation $Dir $Root
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
function Notify([string]$Message) { [void][Windows.Forms.MessageBox]::Show($Message,'TLauncher 1.0.3 TEST') }

$ownerMutex=$null; $locked=$false; $restore=$false; $phase='validation'; $stateFile=$null; $logFile=$null; $failure=$null
function Write-UatLog([string]$Message) {
    if ($logFile) {
        try { Add-Content -LiteralPath $logFile -Value ("{0:o} {1}" -f [DateTime]::UtcNow,$Message) -ErrorAction Stop }
        catch { Write-Warning "Could not write UAT diagnostics: $($_.Exception.Message)" }
    }
}
try {
    $ownerMutex=[Threading.Mutex]::new($false,'Local\TLauncherOwnerUatSwitch')
    try { $locked=$ownerMutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $locked=$true }
    if (-not $locked) { throw 'A UAT switch is already in progress.' }
    if ($UseVerifiedManifest) {
        $CandidateDir=$PSScriptRoot
        $manifest=Join-Path $CandidateDir 'verified-artifact.json'
        Assert-NoReparse $manifest
        $verified=Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
        $ExpectedCommit=$verified.sourceCommit
        $ExpectedExeSha256=$verified.exeSha256
        $ProfileDir=$verified.profile
        $ProductionExe=$verified.productionExe
    }
    $exe=Join-Path $CandidateDir 'TLauncher.exe'
    $root=[IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($CandidateDir))
    if (-not (Same-Path $root (Join-Path $env:LOCALAPPDATA 'TLauncher-UAT')) -and
        -not ($UseVerifiedManifest -and [IO.Path]::GetFileName($root) -eq 'TLauncher-UAT')) { throw 'Candidate must be in the dedicated UAT directory.' }
    Assert-NoReparse $CandidateDir
    if (-not $PreflightOnly) {
        $safeLog=Join-Path $CandidateDir 'uat-launch.log'
        Assert-NoReparse $safeLog
        $logFile=$safeLog
        Write-UatLog 'UAT launcher entered.'
    }
    if (-not $RestoreOnly) { $exe=Assert-Candidate $CandidateDir $ExpectedCommit $ExpectedExeSha256 }
    Assert-NoReparse $ProductionExe
    $prodFull=[IO.Path]::GetFullPath($ProductionExe)
    if (-not (Test-Path -LiteralPath $ProductionExe -PathType Leaf) -or
        $prodFull.StartsWith($root+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Production path is missing or is inside the UAT directory.' }
    if (-not $RestoreOnly) { Test-ProfileLocation $ProfileDir $root }
    if ($PreflightOnly) {
        if ($RestoreOnly) { throw 'Preflight cannot restore production.' }
        Write-Output 'OWNER_UAT_PREFLIGHT=PASS'
        return
    }
    $stateFile=Join-Path $CandidateDir 'uat-state.json'
    Assert-NoReparse $stateFile
    $sessionLog=Join-Path $CandidateDir 'uat-session.log'
    Assert-NoReparse $sessionLog
    $plan=Get-LaunchPlan @(Get-SessionProcesses) $ProductionExe $exe
    if ($RestoreOnly) {
        if ($plan.WasRunning) { Notify 'Production is already running.'; return }
        if (-not (Test-Path -LiteralPath $stateFile)) { throw 'No saved production recovery state.' }
        $saved=Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json
        if (-not $saved.wasRunning -or -not (Same-Path $saved.production $ProductionExe)) { throw 'No verified production session to restore.' }
        $restore=$true
    } else {
        @{wasRunning=$plan.WasRunning;production=$ProductionExe;workingDirectory=[IO.Path]::GetDirectoryName($ProductionExe);commit=$ExpectedCommit;phase='switching'} | ConvertTo-Json | Set-Content -LiteralPath $stateFile -Encoding UTF8
        $phase='stopping production'
        if ($plan.WasRunning) {
            # Verify the same PID, creation timestamp and executable path before
            # invoking the existing singleton quit command. Never kill by name.
            $now=Get-LaunchPlan @(Get-SessionProcesses) $ProductionExe $exe
            if (-not $now.WasRunning -or $now.Main.ProcessId -ne $plan.Main.ProcessId -or $now.Main.CreationDate -ne $plan.Main.CreationDate) { throw 'Production identity changed; aborting switch.' }
            $restore=$true
            Write-UatLog "Requesting normal production exit (PID $($now.Main.ProcessId))."
            Start-Process -FilePath $ProductionExe -ArgumentList '--quit' -WorkingDirectory ([IO.Path]::GetDirectoryName($ProductionExe)) -WindowStyle Hidden -Wait
        }
        if (-not (Wait-Vacant)) { throw 'Production did not exit safely or the singleton is busy. No process was forcibly terminated.' }
        $phase='isolated profile'
        Initialize-Profile $ProfileDir $root
        $phase='candidate launch'
        $process=Start-Process -FilePath $exe -ArgumentList ('--trial-dir "'+$ProfileDir+'"') -WorkingDirectory $CandidateDir -PassThru
        $started=[DateTime]::UtcNow
        Add-Content -LiteralPath $sessionLog -Value ("{0:o} candidate PID={1} commit={2}" -f $started,$process.Id,$ExpectedCommit)
        $process.WaitForExit()
        Add-Content -LiteralPath $sessionLog -Value ("{0:o} candidate exit={1}" -f [DateTime]::UtcNow,$process.ExitCode)
        if ($process.ExitCode -ne 0 -or ([DateTime]::UtcNow-$started).TotalSeconds -lt 2) { throw 'The candidate stopped unexpectedly.' }
    }
} catch {
    $failure="UAT stopped at $phase. "+$_.Exception.Message
    Write-UatLog $failure
    Write-Warning $failure
    if ($PreflightOnly) { throw }
}
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
        } catch {
            $recovery="Production recovery requires attention. "+$_.Exception.Message
            Write-UatLog $recovery
            Write-Warning $recovery
            $failure=(@($failure,$recovery) | Where-Object {$_}) -join [Environment]::NewLine
        }
    }
    if ($locked -and $ownerMutex) { $ownerMutex.ReleaseMutex() }
    if ($ownerMutex) { $ownerMutex.Dispose() }
}
if ($failure) { Notify $failure; exit 1 }
