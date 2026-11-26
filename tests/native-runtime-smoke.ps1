param([Parameter(Mandatory)][string]$Exe,[Parameter(Mandatory)][string]$Evidence)
$ErrorActionPreference='Stop'
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class NativeSmoke {
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c,string t);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
}
'@
function Wait-Until([scriptblock]$Condition,[int]$Seconds=25,[string]$Label='runtime condition'){
    $until=[DateTime]::UtcNow.AddSeconds($Seconds)
    do{if(& $Condition){return};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt  $until)
    throw "Hosted native timeout: $Label"
}
function Snapshot([string]$Name){
    Add-Type -AssemblyName System.Windows.Forms,System.Drawing
    $r=[Windows.Forms.SystemInformation]::VirtualScreen
    $bmp=[Drawing.Bitmap]::new($r.Width,$r.Height)
    $g=[Drawing.Graphics]::FromImage($bmp)
    try{$g.CopyFromScreen($r.Left,$r.Top,0,0,$r.Size);$bmp.Save((Join-Path $Evidence $Name))}finally{$g.Dispose();$bmp.Dispose()}
}
$profile=Join-Path $env:RUNNER_TEMP 'tlauncher-native-trial-ci-runtime'
New-Item -ItemType Directory -Path $profile -Force | Out-Null
[IO.File]::WriteAllText((Join-Path $profile '.tlauncher-native-trial'),"tlauncher-native isolated profile v1`n")
$env:TLAUNCHER_NATIVE_TRIAL_DIR=$profile
$key='HKCU:\Software\TLauncher\NativeBase\Profiles\tlauncher-native-trial-ci-runtime\Impostazioni'
New-Item -Path $key -Force | Out-Null
New-ItemProperty -Path $key -Name Lingua -PropertyType DWord -Value 1 -Force | Out-Null
function Start-Candidate {
    $p=Start-Process -FilePath $Exe -ArgumentList @('--trial-dir',('"'+$profile+'"'),'--log') -PassThru
    $script:currentCandidate=$p
    Write-Host "Waiting for native message window (PID $($p.Id))"
    Wait-Until { [NativeSmoke]::FindWindow('TLauncher.Native.Messaggi',$null)  -ne  [IntPtr]::Zero } -Label 'main message window'
    Write-Host 'Waiting for detached native Guardian'
    Wait-Until {@(Get-CimInstance Win32_Process -Filter "Name='TLauncher.exe'" | Where-Object {$_.ExecutablePath  -ieq  $Exe  -and  $_.CommandLine  -match  '\s--guardiano\s'}).Count  -eq  1} -Label 'detached Guardian'
    if([NativeSmoke]::FindWindow('TLauncher.Native.DockIcone',$null) -eq [IntPtr]::Zero){throw 'Original Dock not created'}
    return $p
}
$report=Join-Path $Evidence 'native-runtime.txt'
$p=$null;$script:currentCandidate=$null
try {
    $p=Start-Candidate
    if([NativeSmoke]::FindWindow('TLauncher.Native.Notch',$null) -eq [IntPtr]::Zero){throw 'Notch not enabled by default'}
    Snapshot 'native-startup.png'
    $main=[NativeSmoke]::FindWindow('TLauncher.Native.Messaggi',$null)
    [void][NativeSmoke]::PostMessage($main,0x8005,[IntPtr]::Zero,[IntPtr]::Zero)
    Wait-Until {[NativeSmoke]::IsWindowVisible([NativeSmoke]::FindWindow('TLauncher.Native.Launcher',$null))}
    Snapshot 'native-launcher.png'
    [void][NativeSmoke]::PostMessage($main,0x8009,[IntPtr]::Zero,[IntPtr]::Zero)
    Wait-Until {[NativeSmoke]::IsWindowVisible([NativeSmoke]::FindWindow('TLauncher.Native.Impostazioni',$null))}
    Snapshot 'native-settings.png'
    Copy-Item tools/native-resource-diagnostics.ps1 (Join-Path (Split-Path -Parent $Exe) 'native-resource-diagnostics.ps1')
    git rev-parse HEAD | Set-Content (Join-Path (Split-Path -Parent $Exe) 'BUILD-COMMIT.txt')
    & (Join-Path (Split-Path -Parent $Exe) 'native-resource-diagnostics.ps1') -Scenario ci_smoke -Seconds 10
    if($LASTEXITCODE  -ne  0){throw 'Resource sampler failed'}
    Copy-Item (Join-Path (Split-Path -Parent $Exe) 'diagnostics/*.json') $Evidence
    foreach($script in Get-ChildItem tools -Filter '*native*.ps1') {
        $tokens=$null;$errors=$null
        $null=[Management.Automation.Language.Parser]::ParseFile($script.FullName,[ref]$tokens,[ref]$errors)
        if($errors.Count){throw ($errors|Out-String)}
    }
    [void][NativeSmoke]::PostMessage([NativeSmoke]::FindWindow('TLauncher.Native.Messaggi',$null),0x10,[IntPtr]::Zero,[IntPtr]::Zero)
    if(!$p.WaitForExit(30000)){throw 'Normal native exit did not finish'}
    Wait-Until {@(Get-CimInstance Win32_Process -Filter "Name='TLauncher.exe'"|Where-Object ExecutablePath -IEQ $Exe).Count -eq  0}
    'NATIVE_STARTUP=PASS','DOCK_WINDOW=PASS','ORIGINAL_LAUNCHER_OPEN=PASS','NATIVE_SETTINGS_OPEN=PASS','NOTCH_DEFAULT_ON=PASS','NATIVE_NORMAL_EXIT_AND_GUARDIAN_EXIT=PASS'|Set-Content $report
    # Saved true-disable survives a real process restart, no transient Notch.
    New-ItemProperty -Path $key -Name NotchEnabled -PropertyType DWord -Value 0 -Force|Out-Null
    $p=Start-Candidate
    if([NativeSmoke]::FindWindow('TLauncher.Native.Notch',$null) -ne [IntPtr]::Zero){throw 'Disabled Notch window exists'}
    # Hosted crash exercise: only this exact CI process; never on owner hardware.
    Stop-Process -Id $p.Id -Force
    Wait-Until {@(Get-CimInstance Win32_Process -Filter "Name='TLauncher.exe'"|Where-Object ExecutablePath -IEQ $Exe).Count -eq  0} 40
    $bar=[NativeSmoke]::FindWindow('Shell_TrayWnd',$null)
    if($bar -ne [IntPtr]::Zero -and  ![NativeSmoke]::IsWindowVisible($bar)){throw 'Guardian did not restore taskbar'}
    'NOTCH_DISABLED_STARTUP=PASS','CRASH_GUARDIAN_EXIT=PASS','TASKBAR_RECOVERY=PASS','OWNER_VISUAL_ACCEPTANCE=PENDING','OWNER_PERFORMANCE_ACCEPTANCE=PENDING'|Add-Content $report
}finally{
    Get-CimInstance Win32_Process -Filter "Name='TLauncher.exe'"|Select-Object ProcessId,ParentProcessId,ExecutablePath,CommandLine|ConvertTo-Json|Set-Content (Join-Path $Evidence 'native-processes.json')
    if(!$p){$p=$script:currentCandidate}
    if($p -and  !$p.HasExited){[void][NativeSmoke]::PostMessage([NativeSmoke]::FindWindow('TLauncher.Native.Messaggi',$null),0x10,[IntPtr]::Zero,[IntPtr]::Zero);$null=$p.WaitForExit(30000)}
    if(Test-Path "$env:TEMP\tlauncher-native.log"){Copy-Item "$env:TEMP\tlauncher-native.log" (Join-Path $Evidence 'native-runtime.log')}
}
