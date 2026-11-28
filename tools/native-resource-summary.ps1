# Pure summary helpers; dot-sourcing performs no process query or sampling.
function Get-NativeSampleStatistics($values) {
    $ordered=@($values | Where-Object { $null -ne $_ })
    if (!$ordered.Count) { return $null }
    $v=@($ordered | Sort-Object)
    [pscustomobject]@{ peak=$v[-1]; average=($v | Measure-Object -Average).Average; p95=$v[[Math]::Max(0,[Math]::Ceiling($v.Count*0.95)-1)]; steadyState=($ordered | Select-Object -Last 5 | Measure-Object -Average).Average; samples=$v.Count }
}
function Get-NativeResourceSummary([object[]]$Rows) {
$totals = @(foreach($row in $rows) {
    if (!$row.processes.Count) { continue }
    $cpu = @($row.processes | Where-Object { $null -ne $_.cpuPercent })
    $pws = @($row.processes | Where-Object { $null -ne $_.privateWorkingSetBytes })
    [pscustomobject]@{
        workingSetMiB=($row.processes | Measure-Object workingSetBytes -Sum).Sum/1MB
        privateMiB=($row.processes | Measure-Object privateBytes -Sum).Sum/1MB
        privateWorkingSetMiB=$(if($pws.Count -eq $row.processes.Count){($pws | Measure-Object privateWorkingSetBytes -Sum).Sum/1MB}else{$null})
        threads=($row.processes | Measure-Object threads -Sum).Sum
        handles=($row.processes | Measure-Object handles -Sum).Sum
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
    [pscustomobject]@{adapter=$_.Name;busiestEnginePercent=(Get-NativeSampleStatistics $_.Group.utilizationPercent)}
})
$gpuMemoryRows = @(foreach($row in $rows){
    if($null -eq $row.gpuProcessMemory){continue}
    $perAdapter = @($row.gpuProcessMemory | ForEach-Object {
        if($_.instance -match '^pid_\d+_(luid_.+?_phys_\d+)$'){
            [pscustomobject]@{adapter=$Matches[1];dedicatedBytes=$_.dedicatedBytes;sharedBytes=$_.sharedBytes}
        }
    })
    foreach($adapter in @($perAdapter | Group-Object adapter)){
        [pscustomobject]@{seconds=$row.seconds;adapter=$adapter.Name;
            dedicatedMiB=($adapter.Group | Measure-Object dedicatedBytes -Sum).Sum/1MB
            sharedMiB=($adapter.Group | Measure-Object sharedBytes -Sum).Sum/1MB}
    }
})
$gpuMemoryStats = @($gpuMemoryRows | Group-Object adapter | ForEach-Object {
    [pscustomobject]@{adapter=$_.Name;dedicatedMiB=(Get-NativeSampleStatistics $_.Group.dedicatedMiB);sharedMiB=(Get-NativeSampleStatistics $_.Group.sharedMiB)}
})
[pscustomobject]@{
    sampleCount=$rows.Count;workingSetMiB=(Get-NativeSampleStatistics $totals.workingSetMiB)
    privateBytesMiB=(Get-NativeSampleStatistics $totals.privateMiB);privateWorkingSetMiB=(Get-NativeSampleStatistics $totals.privateWorkingSetMiB)
    cpuPercent=(Get-NativeSampleStatistics $totals.cpuPercent);threads=(Get-NativeSampleStatistics $totals.threads);handles=(Get-NativeSampleStatistics $totals.handles)
    gpuPerAdapter=$gpuStats;gpuCombinedEngineSamples=$gpuRows;gpuMemoryPerAdapter=$gpuMemoryStats;gpuMemorySamples=$gpuMemoryRows
}
}
