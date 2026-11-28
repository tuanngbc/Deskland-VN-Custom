# Hosted-only fixture: no process enumeration, GPU query or benchmark.
$ErrorActionPreference='Stop'
if($env:GITHUB_ACTIONS -ne 'true'){throw 'Hosted verification only'}
. (Join-Path $PSScriptRoot '../tools/native-resource-summary.ps1')
$processes=@(
 [pscustomobject]@{workingSetBytes=100MB;privateBytes=80MB;privateWorkingSetBytes=60MB;cpuPercent=1;threads=4;handles=100},
 [pscustomobject]@{workingSetBytes=20MB;privateBytes=10MB;privateWorkingSetBytes=8MB;cpuPercent=.2;threads=2;handles=30})
$sample=[pscustomobject]@{seconds=2;processes=$processes;gpuEngines=@(
 [pscustomobject]@{engine='pid_1_luid_A_phys_0_eng_0_engtype_3D';utilizationPercent=2},
 [pscustomobject]@{engine='pid_2_luid_A_phys_0_eng_0_engtype_3D';utilizationPercent=3},
 [pscustomobject]@{engine='pid_1_luid_A_phys_0_eng_1_engtype_Copy';utilizationPercent=4});gpuProcessMemory=@(
 [pscustomobject]@{instance='pid_1_luid_A_phys_0';dedicatedBytes=30MB;sharedBytes=4MB},
 [pscustomobject]@{instance='pid_2_luid_A_phys_0';dedicatedBytes=10MB;sharedBytes=2MB})}
$result=Get-NativeResourceSummary -Rows @($sample)
if($result.workingSetMiB.average -ne 120 -or $result.privateWorkingSetMiB.average -ne 68 -or $result.threads.average -ne 6 -or $result.handles.average -ne 130){throw 'Process total mismatch'}
if($result.gpuPerAdapter.Count -ne 1 -or $result.gpuPerAdapter[0].busiestEnginePercent.average -ne 5){throw 'GPU engines must combine matching PIDs, not add unlike engines'}
if($result.gpuMemoryPerAdapter[0].dedicatedMiB.average -ne 40 -or $result.gpuMemoryPerAdapter[0].sharedMiB.average -ne 6){throw 'GPU memory total mismatch'}
$sample.gpuEngines=$null;$sample.gpuProcessMemory=$null;$processes[1].cpuPercent=$null;$processes[1].privateWorkingSetBytes=$null
$result=Get-NativeResourceSummary -Rows @($sample)
if($null -ne $result.cpuPercent -or $null -ne $result.privateWorkingSetMiB -or $result.gpuMemoryPerAdapter.Count -or $result.gpuPerAdapter.Count){throw 'Unavailable counters must not become zero or partial totals'}
'RESOURCE_PROCESS_AND_GPU_ADAPTER_TOTALS=PASS','UNAVAILABLE_COUNTERS_REMAIN_UNAVAILABLE=PASS'
