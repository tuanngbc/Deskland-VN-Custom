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
            if (!$owned.ContainsKey($id) -and $owned.ContainsKey($parent) -and $entry.CreationDate -ge $owned[$parent].CreationDate) {
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
function Stats($values) {
    $ordered=@($values | Where-Object { $null -ne $_ })
    if (!$ordered.Count) { return $null }
    $v=@($ordered | Sort-Object)
    [pscustomobject]@{ peak=$v[-1]; average=($v | Measure-Object -Average).Average; p95=$v[[Math]::Max(0,[Math]::Ceiling($v.Count*0.95)-1)]; steadyState=($ordered | Select-Object -Last 5 | Measure-Object -Average).Average; samples=$v.Count }
}
$totals = @(foreach($row in $rows) {
    if (!$row.processes.Count) { continue }
    $cpu = @($row.processes | Where-Object { $null -ne $_.cpuPercent })
    $pws = @($row.processes | Where-Object { $null -ne $_.privateWorkingSetBytes })
    [pscustomobject]@{
        workingSetMiB=($row.processes | Measure-Object workingSetBytes -Sum).Sum/1MB
        privateMiB=($row.processes | Measure-Object privateBytes -Sum).Sum/1MB
        privateWorkingSetMiB=$(if($pws.Count -eq $row.processes.Count){($pws | Measure-Object privateWorkingSetBytes -Sum).Sum/1MB}else{$null})
        cpuPercent=$(if($cpu.Count -eq $row.processes.Count){($cpu | Measure-Object cpuPercent -Sum).Sum}else{$null})
    }
})
# Combine all owned PIDs on each physical GPU engine, then take the busiest
# engine on that adapter. Unlike adding 3D+Copy+Video, this stays within 100%.
$gpuRows = @(foreach ($row in $rows) {
    if ($null -eq $row.gpuEngines -or @($row.gpuEngines).Count -eq 0) { continue }
    $engines = @($row.gpuEngines | ForEach-Object {
        if ($_.engine -match '^pid_\d+_(luid_.+?_phys_\d+)_eng_(\d+)_') {
            [pscustomobject]@{adapter=$Matches[1];engine=$Matches[1]+'/eng_'+$Matches[2];value=$_.utilizationPercent}
        }
    })
    foreach ($adapter in @($engines | Group-Object adapter)) {
        $loads = @($adapter.Group | Group-Object engine | ForEach-Object {
            [Math]::Min(100,($_.Group | Measure-Object value -Sum).Sum)
        })
        [pscustomobject]@{seconds=$row.seconds;adapter=$adapter.Name;utilizationPercent=($loads|Measure-Object -Maximum).Maximum}
    }
})
$gpuStats = @($gpuRows | Group-Object adapter | ForEach-Object {
    [pscustomobject]@{adapter=$_.Name;busiestEnginePercent=(Stats $_.Group.utilizationPercent)}
})
$report = [pscustomobject]@{
    scenario=$Scenario; commit=(Get-Content -LiteralPath (Join-Path $candidateDir 'BUILD-COMMIT.txt') -Raw).Trim()
    scope='Exact TEST main, exact-executable/session/owner-PID detached Guardian, and observed descendants. Working-set sum can double-count shared pages; private working set avoids shared-page sum. DWM and sampler excluded. GPU is busiest combined physical engine per adapter, not summed across unlike engines/adapters. Null means unavailable. CI verifies the sampler with the native main and detached Guardian. Owner measurements remain opt-in.'
    sampleCount=$rows.Count; workingSetMiB=(Stats $totals.workingSetMiB); privateBytesMiB=(Stats $totals.privateMiB)
    privateWorkingSetMiB=(Stats $totals.privateWorkingSetMiB); cpuPercent=(Stats $totals.cpuPercent); gpuPerAdapter=$gpuStats; gpuCombinedEngineSamples=$gpuRows; samples=$rows
}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $out -Encoding UTF8
Write-Host "Saved: $out"
$report | Select-Object scenario,sampleCount,workingSetMiB,privateBytesMiB,privateWorkingSetMiB,cpuPercent,gpuPerAdapter | Format-List
