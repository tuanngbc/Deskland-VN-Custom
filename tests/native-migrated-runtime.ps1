# Actual product entry point with the customized, migrated profile. Hosted only.
param([Parameter(Mandatory)][string]$Exe,[Parameter(Mandatory)][string]$Evidence,[Parameter(Mandatory)][string]$Mesa)
$ErrorActionPreference='Stop'
if($env:GITHUB_ACTIONS -ne 'true'){throw 'Hosted verification only.'}
Add-Type @"
using System;using System.Runtime.InteropServices;
public static class MigratedRuntime {
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c,string t);
 public static IntPtr Window(string c){return FindWindow(c,null);}
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,System.Text.StringBuilder s,int n);
 [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr h,EnumProc p,IntPtr l);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p,IntPtr l);
 delegate bool EnumProc(IntPtr h,IntPtr l);
 public static int VisibleWallpaper(){int n=0;EnumProc count=(h,l)=>{var s=new System.Text.StringBuilder(128);GetClassName(h,s,128);if(s.ToString()=="TLauncher.Native.Wallpaper"&&IsWindowVisible(h))n++;return true;};EnumWindows((h,l)=>{count(h,l);EnumChildWindows(h,count,IntPtr.Zero);return true;},IntPtr.Zero);return n;}
}
"@
function Wait-Migrated([scriptblock]$Condition,[string]$Label){
 $until=[DateTime]::UtcNow.AddSeconds(45)
 do{if($script:migratedCandidate -and $script:migratedCandidate.HasExited){throw "Product exited: $($script:migratedCandidate.ExitCode) ($Label)"};if(& $Condition){return};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $until)
 throw "Migrated runtime timeout: $Label"
}
function Get-MigratedProcesses { @(Get-CimInstance Win32_Process -Filter "Name='TLauncher.exe'"|Where-Object ExecutablePath -IEQ $runtimeExe) }
function Read-MigratedLog {
 $stream=$null;$reader=$null
 try{
  $stream=[IO.File]::Open((Join-Path $env:TEMP 'tlauncher-native.log'),[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
  $reader=[IO.StreamReader]::new($stream);return $reader.ReadToEnd()
 }catch [IO.IOException]{return ''}finally{if($reader){$reader.Dispose()}elseif($stream){$stream.Dispose()}}
}
$manifest=Get-Content (Join-Path $Evidence 'profile-migration-runtime.json') -Raw|ConvertFrom-Json
if($manifest.commit -cne $env:GITHUB_SHA -or !(Test-Path (Join-Path $manifest.profile '.tlauncher-native-trial'))){throw 'Migrated fixture identity mismatch.'}
if([MigratedRuntime]::Window('TLauncher.Native.Messaggi') -ne [IntPtr]::Zero){throw 'Another native shell is active.'}
$runtime=Join-Path $env:RUNNER_TEMP 'migrated-native-product-runtime'
[IO.Directory]::CreateDirectory($runtime)|Out-Null
Copy-Item $Exe (Join-Path $runtime 'TLauncher.exe')
Copy-Item (Join-Path (Split-Path -Parent $Exe) 'wallpaper') $runtime -Recurse
Copy-Item (Join-Path $Mesa 'x64/*.dll') $runtime
$runtimeExe=Join-Path $runtime 'TLauncher.exe'
$priorProfile=$env:TLAUNCHER_NATIVE_TRIAL_DIR;$priorDriver=$env:GALLIUM_DRIVER;$priorSoftware=$env:LIBGL_ALWAYS_SOFTWARE
$env:TLAUNCHER_NATIVE_TRIAL_DIR=$manifest.profile;$env:GALLIUM_DRIVER='llvmpipe';$env:LIBGL_ALWAYS_SOFTWARE='true'
$script:migratedCandidate=$null
try{
 foreach($launch in 'migrated-first-start','migrated-restart'){
  $logPath=Join-Path $env:TEMP 'tlauncher-native.log';if(Test-Path $logPath){Remove-Item -LiteralPath $logPath}
  $script:migratedCandidate=Start-Process -FilePath $runtimeExe -WorkingDirectory $runtime -ArgumentList @('--trial-dir',('"'+$manifest.profile+'"'),'--log') -WindowStyle Hidden -PassThru
  Wait-Migrated { [MigratedRuntime]::Window('TLauncher.Native.Messaggi') -ne [IntPtr]::Zero } 'normal entry point'
  Wait-Migrated { (Read-MigratedLog) -match 'PROFILE_RESTORED language=7 dock_visibility=2 notch=0 stage_enabled=1 stage_height=174 peek_delay=750 spotlight_mode=2 spotlight_mods=3 spotlight_key=133 stage_mods=5 stage_key=83 stage_shortcut=0' } 'actual restored native settings'
  Wait-Migrated { (Read-MigratedLog) -match 'WALLPAPER_RESTORED enabled=1 scene=lake fps=29 layout=2 resolution=0.75 pause_covered=0 weather=5 clock=0 hour=17.25 wind=1.40 assignments=[1-9][0-9]* depth=321.00' } 'actual restored wallpaper settings'
  if([MigratedRuntime]::Window('TLauncher.Native.Notch') -ne [IntPtr]::Zero){throw 'Migrated disabled Notch was created.'}
  if([MigratedRuntime]::Window('TLauncher.Native.Stage') -eq [IntPtr]::Zero){throw 'Migrated enabled Stage was not created.'}
  Wait-Migrated { [MigratedRuntime]::VisibleWallpaper() -gt 0 } 'restored wallpaper presentation'
  $main=[MigratedRuntime]::Window('TLauncher.Native.Messaggi')
  [void][MigratedRuntime]::PostMessage($main,0x8008,[IntPtr]::Zero,[IntPtr]::Zero)
  Wait-Migrated { [MigratedRuntime]::IsWindowVisible([MigratedRuntime]::Window('TLauncher.Native.Ricerca')) } 'restored Spotlight presentation'
  if($launch -eq 'migrated-first-start'){
   Add-Type -AssemblyName System.Windows.Forms,System.Drawing
   $rect=[Windows.Forms.SystemInformation]::VirtualScreen;$bmp=[Drawing.Bitmap]::new($rect.Width,$rect.Height);$graphics=[Drawing.Graphics]::FromImage($bmp)
   try{$graphics.CopyFromScreen($rect.Left,$rect.Top,0,0,$rect.Size);$bmp.Save((Join-Path $Evidence 'migrated-product-spotlight.png'))}finally{$graphics.Dispose();$bmp.Dispose()}
  }
  [void][MigratedRuntime]::PostMessage($main,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
  if(!$script:migratedCandidate.WaitForExit(30000)){throw 'Migrated product normal exit failed.'}
  $script:migratedCandidate=$null
  Wait-Migrated { (Get-MigratedProcesses).Count -eq 0 } 'Guardian and product vacancy'
  Copy-Item (Join-Path $env:TEMP 'tlauncher-native.log') (Join-Path $Evidence ($launch+'.log'))
 }
 'MIGRATED_FULL_PRODUCT_START=PASS','MIGRATED_CUSTOM_WALLPAPER_PRESENT=PASS','MIGRATED_NATIVE_STAGE_AND_SPOTLIGHT=PASS','MIGRATED_NOTCH_DISABLED=PASS','MIGRATED_FULL_PRODUCT_RESTART=PASS','MIGRATED_NORMAL_EXIT_AND_GUARDIAN_EXIT=PASS','OWNER_SETTINGS_RESTORED=NOT_RUN_ON_OWNER'|Set-Content (Join-Path $Evidence 'native-migrated-runtime.txt')
}finally{
 Add-Type -AssemblyName System.Windows.Forms,System.Drawing
 $lastRect=[Windows.Forms.SystemInformation]::VirtualScreen;$lastBmp=[Drawing.Bitmap]::new($lastRect.Width,$lastRect.Height);$lastGraphics=[Drawing.Graphics]::FromImage($lastBmp)
 try{$lastGraphics.CopyFromScreen($lastRect.Left,$lastRect.Top,0,0,$lastRect.Size);$lastBmp.Save((Join-Path $Evidence 'migrated-runtime-last-screen.png'))}finally{$lastGraphics.Dispose();$lastBmp.Dispose()}
 Get-CimInstance Win32_Process -Filter "Name='TLauncher.exe'"|Select-Object ProcessId,ParentProcessId,ExecutablePath,CommandLine|ConvertTo-Json|Set-Content (Join-Path $Evidence 'migrated-runtime-processes.json')
 if(Test-Path (Join-Path $env:TEMP 'tlauncher-native.log')){Copy-Item (Join-Path $env:TEMP 'tlauncher-native.log') (Join-Path $Evidence 'migrated-runtime-last.log')}
 if($script:migratedCandidate -and !$script:migratedCandidate.HasExited){[void][MigratedRuntime]::PostMessage([MigratedRuntime]::Window('TLauncher.Native.Messaggi'),0x10,[IntPtr]::Zero,[IntPtr]::Zero);$null=$script:migratedCandidate.WaitForExit(30000)}
 $env:TLAUNCHER_NATIVE_TRIAL_DIR=$priorProfile;$env:GALLIUM_DRIVER=$priorDriver;$env:LIBGL_ALWAYS_SOFTWARE=$priorSoftware
}
