# Owner opt-in UAT companion. Includes the exact session Guardian plus observed descendants. Prepared, not executed locally.
[CmdletBinding()]
param([string]$Scenario = '', [ValidateRange(10,1800)][int]$Seconds = 60)
$ErrorActionPreference = 'Stop'
$candidateDir = $PSScriptRoot
$exe = Join-Path $candidateDir 'TLauncher.exe'
if (!(Test-Path -LiteralPath $exe)) { throw 'Run from the verified candidate directory.' }
$roots = @(Get-CimInstance Win32_Process -Filter "Name='TLauncher.exe'" |
    Where-Object {
        $_.ExecutablePath -and $_.ExecutablePath -ieq $exe -and $_.CommandLine -and
        $_.CommandLine -notmatch '(?:^|\s)--(?:guardiano|lancia-guardiano|quit)(?:\s|$)'
    })
if ($roots.Count -ne 1) { throw 'Open this candidate using the Desktop TEST shortcut first.' }
$rootId = [int]$roots[0].ProcessId
$rootCreated = $roots[0].CreationDate
$rootSession = $roots[0].SessionId
if (!$Scenario) { $Scenario = Read-Host 'Scenario (desktop, covered, Spotlight, Settings, recording, idle)' }
$Scenario = ($Scenario -replace '[^a-zA-Z0-9_-]','_').Substring(0,[Math]::Min(60,$Scenario.Length))
if (!$Scenario) { $Scenario = 'desktop' }
$folder = Join-Path $PSScriptRoot 'diagnostics'
New-Item -ItemType Directory -Force -Path $folder | Out-Null
$out = Join-Path $folder ("{0}-{1}.json" -f (Get-Date -Format 'yyyyMMdd-HHmmss'),$Scenario)
$rows = [Collections.Generic.List[object]]::new()
$previous = @{}
$clock = [Diagnostics.Stopwatch]::StartNew()
$previousAt = 0.0
Write-Host "Sampling this candidate for $Seconds seconds. Continue normal use. No screen capture or upload."
while ($clock.Elapsed.TotalSeconds -lt $Seconds) {
    $table = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,CreationDate,Name,ExecutablePath,CommandLine,SessionId)
    $root = $table | Where-Object { $_.ProcessId -eq $rootId -and $_.CreationDate -eq $rootCreated }
    if (!$root) { break }
    $owned = @{$rootId=$root}
    # The detached Guardian outlives its bootstrap parent. Match its owner PID
    # with exact candidate executable/session and a birth after the live main.
    foreach ($guard in $table) {
        if ($guard.ExecutablePath -ieq $exe -and $guard.SessionId -eq $rootSession -and
            $guard.CreationDate -ge $rootCreated -and $guard.CommandLine -match
            ('(?:^|\s)--guardiano\s+'+$rootId+'\s')) {
            $owned[[int]$guard.ProcessId]=$guard
        }
    }
    do {
        $changed = $false
        foreach ($entry in $table) {
            $id = [int]$entry.ProcessId; $parent = [int]$entry.ParentProcessId
            # Apps opened by the Launcher are user workloads, not TLauncher
            # helpers. Include only packaged executables and the upstream cursor
            # archive helper, not Chrome/Explorer/etc launched from the Dock.
            $helper=$entry.ExecutablePath -and (
                $entry.ExecutablePath.StartsWith($candidateDir+'\',[StringComparison]::OrdinalIgnoreCase) -or
                $entry.ExecutablePath -ieq (Join-Path $env:SystemRoot 'System32\tar.exe'))
            if ($helper -and !$owned.ContainsKey($id) -and $owned.ContainsKey($parent) -and $entry.CreationDate -ge $owned[$parent].CreationDate) {
                $owned[$id] = $entry; $changed = $true
            }
        }
    } while ($changed)
    $at = $clock.Elapsed.TotalSeconds
    $privateWs = @{}
    try {
        foreach ($x in (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -ErrorAction Stop)) {
            $privateWs[[int]$x.IDProcess] = [long]$x.WorkingSetPrivate
        }
    } catch {}
    $gpuEngines = @(); $gpuMemory = @()
    try {
        $gpuEngines = @(Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine -ErrorAction Stop |
            Where-Object { $_.Name -match '^pid_(\d+)_' -and $owned.ContainsKey([int]$Matches[1]) } |
            ForEach-Object { [pscustomobject]@{ engine=$_.Name; utilizationPercent=[double]$_.UtilizationPercentage } })
    } catch { $gpuEngines = $null }
    try {
        $gpuMemory = @(Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUProcessMemory -ErrorAction Stop |
            Where-Object { $_.Name -match '^pid_(\d+)_' -and $owned.ContainsKey([int]$Matches[1]) } |
            ForEach-Object { [pscustomobject]@{ instance=$_.Name; dedicatedBytes=[long]$_.DedicatedUsage; sharedBytes=[long]$_.SharedUsage } })
    } catch { $gpuMemory = $null }
    $next = @{}
    $processes = @(foreach ($id in $owned.Keys) {
        $proc = Get-Process -Id $id -ErrorAction SilentlyContinue
        if (!$proc) { continue }
        try {
            $key = "$id/$($proc.StartTime.Ticks)"
            $cpu = $proc.TotalProcessorTime.TotalSeconds
            $next[$key] = $cpu
            $cpuPercent = $null
            if ($previous.ContainsKey($key) -and $at -gt $previousAt) {
                $cpuPercent = [Math]::Max(0,100*($cpu-$previous[$key])/($at-$previousAt)/[Environment]::ProcessorCount)
            }
            [pscustomobject]@{
                pid=$id; name=$proc.ProcessName; isMain=($id -eq $rootId)
                isGuardian=($owned[$id].CommandLine -match '(?:^|\s)--guardiano(?:\s|$)')
                workingSetBytes=$proc.WorkingSet64; privateBytes=$proc.PrivateMemorySize64
                privateWorkingSetBytes=$(if($privateWs.ContainsKey($id)){$privateWs[$id]}else{$null})
                cpuPercent=$cpuPercent; threads=$proc.Threads.Count; handles=$proc.HandleCount
            }
        } catch {} finally { $proc.Dispose() }
    })
    $rows.Add([pscustomobject]@{ seconds=[Math]::Round($at,2); processes=$processes; gpuEngines=$gpuEngines; gpuProcessMemory=$gpuMemory })
    $previous=$next; $previousAt=$at
    Start-Sleep -Seconds 2
}
. (Join-Path $PSScriptRoot 'native-resource-summary.ps1')
$summary=Get-NativeResourceSummary -Rows $rows.ToArray()
$report = [pscustomobject]@{
    scenario=$Scenario; commit=(Get-Content -LiteralPath (Join-Path $candidateDir 'BUILD-COMMIT.txt') -Raw).Trim()
    scope='Exact TEST main, detached Guardian matched to live owner PID/path/session, and observed packaged/cursor archive helpers. User apps launched from Dock, DWM and sampler excluded. Working-set sum can double-count shared pages. GPU is busiest combined physical engine per adapter, not summed across unlike engines/adapters. GPU memory sums process counters per adapter; shared allocations may be counted by multiple processes. Null means unavailable. Owner measurements remain opt-in.'
    samples=$rows
}
foreach($property in $summary.PSObject.Properties){$report | Add-Member -NotePropertyName $property.Name -NotePropertyValue $property.Value}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $out -Encoding UTF8
Write-Host "Saved: $out"
$report | Select-Object scenario,sampleCount,workingSetMiB,privateBytesMiB,privateWorkingSetMiB,cpuPercent,threads,handles,gpuPerAdapter,gpuMemoryPerAdapter | Format-List
