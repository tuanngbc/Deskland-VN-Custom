# Untouched FluidWin binary, GitHub-hosted runner only. Never invoke on owner.
param([Parameter(Mandatory)][string]$Exe,[Parameter(Mandatory)][string]$Evidence)
$ErrorActionPreference='Stop'
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class UpstreamSmoke {
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c,string t);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
}
'@
function Wait-Upstream([scriptblock]$Condition){
 $until=[DateTime]::UtcNow.AddSeconds(25)
 do{if(& $Condition){return};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $until)
 throw 'Untouched upstream runtime condition timed out'
}
$full=(Resolve-Path $Exe).Path;$leaf=[IO.Path]::GetFileName($full)
$p=Start-Process -FilePath $full -ArgumentList '--log' -PassThru -WindowStyle Hidden
try{
 Wait-Upstream {[UpstreamSmoke]::FindWindow('FluidWin.Messaggi',$null) -ne [IntPtr]::Zero}
 Wait-Upstream {@(Get-CimInstance Win32_Process -Filter "Name='$leaf'"|Where-Object {$_.ExecutablePath -ieq $full -and $_.CommandLine -match '\s--guardiano\s'}).Count -eq 1}
 foreach($class in @('FluidWin.DockIcone','FluidWin.Notch','FluidWin.Launcher')){if([UpstreamSmoke]::FindWindow($class,$null) -eq [IntPtr]::Zero){throw "Missing upstream $class"}}
 $main=[UpstreamSmoke]::FindWindow('FluidWin.Messaggi',$null)
 [void][UpstreamSmoke]::PostMessage($main,0x8005,[IntPtr]::Zero,[IntPtr]::Zero)
 Wait-Upstream {[UpstreamSmoke]::IsWindowVisible([UpstreamSmoke]::FindWindow('FluidWin.Launcher',$null))}
 [void][UpstreamSmoke]::PostMessage($main,0x8009,[IntPtr]::Zero,[IntPtr]::Zero)
 Wait-Upstream {[UpstreamSmoke]::FindWindow('FluidWin.Impostazioni',$null) -ne [IntPtr]::Zero}
 [void][UpstreamSmoke]::PostMessage($main,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
 if(!$p.WaitForExit(30000)){throw 'Original application did not exit normally'}
 Wait-Upstream {@(Get-CimInstance Win32_Process -Filter "Name='$leaf'"|Where-Object ExecutablePath -IEQ $full).Count -eq 0}
 'UNTOUCHED_UPSTREAM_STARTUP=PASS','ORIGINAL_DOCK_NOTCH_LAUNCHER=PASS','ORIGINAL_LAUNCHER_OPEN=PASS','ORIGINAL_SETTINGS_OPEN=PASS','ORIGINAL_GUARDIAN_EXIT=PASS','OWNER_VISUAL_QUALITY=NOT_MEASURED'|Set-Content (Join-Path $Evidence 'upstream-runtime.txt')
}finally{
 Get-CimInstance Win32_Process -Filter "Name='$leaf'"|Select-Object ProcessId,ParentProcessId,ExecutablePath,CommandLine|ConvertTo-Json|Set-Content (Join-Path $Evidence 'upstream-processes.json')
 if(!$p.HasExited){[void][UpstreamSmoke]::PostMessage([UpstreamSmoke]::FindWindow('FluidWin.Messaggi',$null),0x10,[IntPtr]::Zero,[IntPtr]::Zero);$null=$p.WaitForExit(30000)}
 if(Test-Path "$env:TEMP\fluidwin.log"){Copy-Item "$env:TEMP\fluidwin.log" (Join-Path $Evidence 'upstream-runtime.log')}
}
