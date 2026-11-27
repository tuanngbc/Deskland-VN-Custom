# Full native executable, hosted only. Serial baseline/off/on comparison.
param([Parameter(Mandatory)][string]$Exe,[Parameter(Mandatory)][string]$Evidence,[Parameter(Mandatory)][string]$Mesa)
$ErrorActionPreference='Stop'
if($env:GITHUB_ACTIONS -ne 'true'){throw 'Hosted verification only'}
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class WallpaperIntegration {
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c,string t);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,System.Text.StringBuilder s,int n);
 [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr h,EnumProc p,IntPtr l);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p,IntPtr l);
 delegate bool EnumProc(IntPtr h,IntPtr l);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll",SetLastError=true)] static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,IntPtr l,uint f,uint ms,out IntPtr r);
 public static bool Responsive(IntPtr h) { IntPtr r; return SendMessageTimeout(h,0,IntPtr.Zero,IntPtr.Zero,2,2000,out r)!=IntPtr.Zero; }
 public static int WallpaperCount(bool visible) { int n=0; EnumProc count=(h,l)=>{var s=new System.Text.StringBuilder(128);GetClassName(h,s,128);if(s.ToString()=="TLauncher.Native.Wallpaper"&&(!visible||IsWindowVisible(h)))n++;return true;}; EnumWindows((h,l)=>{count(h,l);EnumChildWindows(h,count,IntPtr.Zero);return true;},IntPtr.Zero);return n; }
}
'@
function Wait-For([scriptblock]$Check,[string]$Label){$until=[DateTime]::UtcNow.AddSeconds(45);do{if(& $Check){return};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $until);throw "Timeout: $Label"}
function Main-Window { [WallpaperIntegration]::FindWindow('TLauncher.Native.Messaggi',$null) }
function Candidate-Processes([string]$Path){@(Get-CimInstance Win32_Process -Filter "Name='TLauncher.exe'"|Where-Object ExecutablePath -IEQ $Path)}
function Send-Action([IntPtr]$Window,[int]$Action){[void][WallpaperIntegration]::PostMessage($Window,0x805b,[IntPtr]$Action,[IntPtr]::Zero)}
New-Item -ItemType Directory -Path $Evidence -Force|Out-Null
$headers=@{Authorization=('Bearer '+$env:GH_TOKEN);Accept='application/vnd.github+json'}
$api='https://api.github.com/repos/tuanngbc/TLauncher'
$baselineCommit='c011c2b11b3d99e25db5bc66b5f02b51246e78fc'
$run=Invoke-RestMethod "$api/actions/runs/45870002906" -Headers $headers
if($run.head_sha -cne $baselineCommit -or $run.conclusion -cne 'success'){throw 'Accepted baseline identity mismatch'}
$outer=Join-Path $env:RUNNER_TEMP 'accepted-native-baseline.zip'
Invoke-WebRequest "$api/actions/artifacts/15141503112/zip" -Headers $headers -OutFile $outer
if((Get-FileHash $outer).Hash.ToLowerInvariant() -cne '64258020f61f0bb05da95d3936859bc9d881afa1b456aa5c605c1832386f4a0f'){throw 'Accepted baseline artifact checksum'}
$baseRoot=Join-Path $env:RUNNER_TEMP 'accepted-native-reference'
Expand-Archive -LiteralPath $outer -DestinationPath $baseRoot
$inner=@(Get-ChildItem $baseRoot -Recurse -Filter '*.zip');if($inner.Count -ne 1){throw 'Baseline package ambiguous'}
if((Get-FileHash $inner[0].FullName).Hash.ToLowerInvariant() -cne '3d1ba429a7ac6a4f50e3d37b3e448c2526d1ccf9287384cbf6208b9bd1d0d42f'){throw 'Accepted baseline package checksum'}
Expand-Archive -LiteralPath $inner[0].FullName -DestinationPath (Join-Path $baseRoot 'package')
$baseExe=@(Get-ChildItem (Join-Path $baseRoot 'package') -Recurse -Filter TLauncher.exe);if($baseExe.Count -ne 1){throw 'Baseline executable ambiguous'}
if((Get-Content (Join-Path $baseExe[0].DirectoryName 'BUILD-COMMIT.txt') -Raw).Trim() -cne $baselineCommit -or $baseExe[0].VersionInfo.ProductVersion -cne '1.0.3'){throw 'Baseline metadata'}
$results=[Collections.Generic.List[object]]::new()
foreach($mode in 'accepted-baseline','wallpaper-disabled','wallpaper-enabled'){
 $runtime=Join-Path $env:RUNNER_TEMP ('wallpaper-integration-'+$mode);New-Item -ItemType Directory -Path $runtime|Out-Null
 if($mode -eq 'accepted-baseline'){Copy-Item (Join-Path $baseExe[0].DirectoryName '*') $runtime -Recurse}
 else {Copy-Item $Exe (Join-Path $runtime 'TLauncher.exe');Copy-Item (Join-Path (Split-Path $Exe) 'wallpaper') $runtime -Recurse;Copy-Item (Join-Path $Mesa 'x64/*.dll') $runtime}
 $runtimeExe=Join-Path $runtime 'TLauncher.exe'
 Copy-Item tools/native-resource-diagnostics.ps1 $runtime
 $(if($mode -eq 'accepted-baseline'){$baselineCommit}else{$env:GITHUB_SHA})|Set-Content (Join-Path $runtime 'BUILD-COMMIT.txt')
 $profile=Join-Path $env:RUNNER_TEMP ('tlauncher-native-trial-'+$mode);New-Item -ItemType Directory -Path $profile|Out-Null
 [IO.File]::WriteAllText((Join-Path $profile '.tlauncher-native-trial'),"tlauncher-native isolated profile v1`n")
 if($mode -eq 'wallpaper-enabled'){[IO.File]::WriteAllText((Join-Path $profile 'wallpaper-v1.ini'),"schema=1`nenabled=1`nscene=beach`nfps=25`npauseCovered=0`nsolar=1`n")}
 $env:TLAUNCHER_NATIVE_TRIAL_DIR=$profile;$env:GALLIUM_DRIVER='llvmpipe';$env:LIBGL_ALWAYS_SOFTWARE='true'
 $key='HKCU:\Software\TLauncher\NativeBase\Profiles\'+[IO.Path]::GetFileName($profile)+'\Impostazioni'
 New-Item -Path $key -Force|Out-Null;New-ItemProperty -Path $key -Name Lingua -PropertyType DWord -Value 1 -Force|Out-Null
 $startup=[Collections.Generic.List[object]]::new();$candidate=$null
 try{
  foreach($launch in 'first-observed','warm'){
   if((Main-Window) -ne [IntPtr]::Zero){throw 'Another native shell is running'}
   $watch=[Diagnostics.Stopwatch]::StartNew()
   $candidate=Start-Process -FilePath $runtimeExe -ArgumentList @('--trial-dir',('"'+$profile+'"')) -WorkingDirectory $runtime -WindowStyle Hidden -PassThru
   Wait-For { (Main-Window) -ne [IntPtr]::Zero } 'message window'
   $startup.Add([pscustomobject]@{launch=$launch;messageWindowMs=$watch.Elapsed.TotalMilliseconds})
   if($mode -eq 'wallpaper-enabled'){Wait-For {[WallpaperIntegration]::WallpaperCount($true) -gt 0} 'visible wallpaper presentation'}
   elseif([WallpaperIntegration]::WallpaperCount($false) -ne 0){throw 'Disabled module created wallpaper windows'}
   if($launch -eq 'first-observed'){
    [void][WallpaperIntegration]::PostMessage((Main-Window),0x10,[IntPtr]::Zero,[IntPtr]::Zero)
    if(!$candidate.WaitForExit(30000)){throw 'First normal shutdown'}
    Wait-For { (Candidate-Processes $runtimeExe).Count -eq 0 } 'Guardian shutdown'
   }
  }
  $latency=@(1..12|ForEach-Object {$watch=[Diagnostics.Stopwatch]::StartNew();if(![WallpaperIntegration]::Responsive((Main-Window))){throw 'Native UI hung'};$watch.Elapsed.TotalMilliseconds})
  & (Join-Path $runtime 'native-resource-diagnostics.ps1') -Scenario $mode -Seconds 10
  Copy-Item (Join-Path $runtime 'diagnostics/*.json') $Evidence
  if($mode -eq 'wallpaper-enabled'){
   [void][WallpaperIntegration]::PostMessage((Main-Window),0x8009,[IntPtr]::Zero,[IntPtr]::Zero)
   $settings=[WallpaperIntegration]::FindWindow('TLauncher.Native.Impostazioni',$null)
   Wait-For {[WallpaperIntegration]::IsWindowVisible($settings)} 'native Settings'
   Start-Sleep -Milliseconds 700
   1..6|ForEach-Object {[void][WallpaperIntegration]::PostMessage($settings,0x100,[IntPtr]0x28,[IntPtr]::Zero);Start-Sleep -Milliseconds 100}
   Send-Action $settings 5103
   Wait-For {(Get-Content (Join-Path $profile 'wallpaper-v1.ini') -Raw) -match '(?m)^scene=lake\r?$'} 'real Settings scene selection'
   Send-Action $settings 5000
   Wait-For {[WallpaperIntegration]::WallpaperCount($false) -eq 0} 'native disable releases windows'
   Wait-For {(Get-Content (Join-Path $profile 'wallpaper-v1.ini') -Raw) -match '(?m)^enabled=0\r?$'} 'persisted disable'
   Send-Action $settings 5000
   Wait-For {[WallpaperIntegration]::WallpaperCount($true) -gt 0} 'native enable recreates presentation'
   Add-Type -AssemblyName System.Windows.Forms,System.Drawing
   $rect=[Windows.Forms.SystemInformation]::VirtualScreen;$bmp=[Drawing.Bitmap]::new($rect.Width,$rect.Height);$graphics=[Drawing.Graphics]::FromImage($bmp)
   try{$graphics.CopyFromScreen($rect.Left,$rect.Top,0,0,$rect.Size);$bmp.Save((Join-Path $Evidence 'wallpaper-native-settings.png'))}finally{$graphics.Dispose();$bmp.Dispose()}
  }
  $results.Add([pscustomobject]@{mode=$mode;startup=$startup;uiRoundTripMs=$latency;uiRoundTripMaxMs=($latency|Measure-Object -Maximum).Maximum;wallpaperVisible=[WallpaperIntegration]::WallpaperCount($true);resourceReport='See exact main+Guardian JSON';coldStartup='NOT_ESTABLISHED; first-observed does not purge OS caches'})
 }finally{
  if($candidate -and !$candidate.HasExited){[void][WallpaperIntegration]::PostMessage((Main-Window),0x10,[IntPtr]::Zero,[IntPtr]::Zero);if(!$candidate.WaitForExit(30000)){throw 'Native integration normal shutdown failed'}}
  Wait-For {(Candidate-Processes $runtimeExe).Count -eq 0} 'all exact candidate processes exited'
  if([WallpaperIntegration]::WallpaperCount($false) -ne 0){throw 'Wallpaper HWND leaked after shutdown'}
 }
}
$results|ConvertTo-Json -Depth 8|Set-Content (Join-Path $Evidence 'accepted-baseline-comparison.json')
'ACCEPTED_BASELINE_IDENTITY=PASS','SERIAL_BASELINE_DISABLED_ENABLED=PASS','NATIVE_SETTINGS_SCENE_SELECT=PASS','NATIVE_SETTINGS_DISABLE_RELEASE_ENABLE=PASS','NORMAL_EXIT_ALL_PROCESSES=PASS','OWNER_GPU_PERFORMANCE=NOT_MEASURED'|Set-Content (Join-Path $Evidence 'wallpaper-native-integration.txt')
