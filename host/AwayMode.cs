using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Microsoft.Win32;

static class AwayRegistration
{
    internal static readonly string[] Keys={"SCRNSAVE.EXE","ScreenSaveActive","ScreenSaverIsSecure","ScreenSaveTimeOut"};
    public static string SaverExecutable {get{return Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"DesklandSaver.scr");}}
    static string Backup {get{return Path.Combine(SaverPaths.Profile,"saver-registry-backup.json");}}
    public static bool Enabled {
        get {using(var key=Registry.CurrentUser.OpenSubKey(@"Control Panel\Desktop")) return key!=null&&string.Equals(key.GetValue("SCRNSAVE.EXE") as string,SaverExecutable,StringComparison.OrdinalIgnoreCase)&&(key.GetValue("ScreenSaveActive") as string)=="1";}
    }
    internal static void Install(RegistryKey key,string executable,int minutes,string backup)
    {
        if(minutes<1||minutes>120)throw new ArgumentOutOfRangeException("minutes");
        if(!File.Exists(backup)) {
            var previous=new Dictionary<string,object>();
            foreach(string name in Keys) previous[name]=key.GetValue(name,null);
            File.WriteAllText(backup,Json.Stringify(previous),new UTF8Encoding(false));
        }
        key.SetValue("SCRNSAVE.EXE",executable,RegistryValueKind.String);
        key.SetValue("ScreenSaveActive","1",RegistryValueKind.String);
        key.SetValue("ScreenSaverIsSecure","1",RegistryValueKind.String);
        key.SetValue("ScreenSaveTimeOut",(minutes*60).ToString(),RegistryValueKind.String);
    }
    internal static bool Restore(RegistryKey key,string executable,string backup)
    {
        // Respect a screen saver the user selected in Windows after Deskland.
        if(!string.Equals(key.GetValue("SCRNSAVE.EXE") as string,executable,StringComparison.OrdinalIgnoreCase))return false;
        if(!File.Exists(backup))return false;
        var previous=(Dictionary<string,object>)Json.Parse(File.ReadAllText(backup));
        foreach(string name in Keys) {
            object value; if(previous.TryGetValue(name,out value)&&value!=null) key.SetValue(name,value,RegistryValueKind.String);
            else key.DeleteValue(name,false);
        }
        File.Delete(backup);return true;
    }
    public static void Configure(bool enabled,int minutes)
    {
        if(InstallProfile.Isolated)throw new InvalidOperationException("Isolated tests cannot change the Windows screen saver.");
        if(!File.Exists(SaverExecutable))throw new FileNotFoundException("DesklandSaver.scr",SaverExecutable);
        using(var key=Registry.CurrentUser.CreateSubKey(@"Control Panel\Desktop")) {
            if(enabled)Install(key,SaverExecutable,minutes,Backup);
            else Restore(key,SaverExecutable,Backup);
        }
        // Tell Windows to re-read the active user's setting; preserve secure resume.
        int unused=0;
        SaverNative.SystemParametersInfo(0x0011,enabled?1:0,ref unused,0);
        if(enabled) { SaverNative.SystemParametersInfo(0x000F,minutes*60,ref unused,0); SaverNative.SystemParametersInfo(0x0077,1,ref unused,0); }
        else {
            using(var key=Registry.CurrentUser.OpenSubKey(@"Control Panel\Desktop")) {
                bool active=key!=null&&(key.GetValue("ScreenSaveActive") as string)=="1";
                SaverNative.SystemParametersInfo(0x0011,active?1:0,ref unused,0);
                int timeout;if(key!=null&&int.TryParse(key.GetValue("ScreenSaveTimeOut") as string,out timeout))SaverNative.SystemParametersInfo(0x000F,timeout,ref unused,0);
                if(key!=null)SaverNative.SystemParametersInfo(0x0077,(key.GetValue("ScreenSaverIsSecure") as string)=="1"?1:0,ref unused,0);
            }
        }
    }
}

partial class HostContext
{
    readonly SaverPowerWindow powerWindow=new SaverPowerWindow();
    bool awayReleased,cacheBusy,sessionLocked,systemSleeping,displayOff;
    bool reopenAfterAway;
    internal bool AwayReleased {get{return awayReleased;}}
    internal void AwayForTest(bool value) {sessionLocked=value;UpdateAwayState();}
    void UpdateAwayState()
    {
        bool value=sessionLocked||systemSleeping||displayOff||SaverPaths.IsRunning();
        sessionPaused=value;
        if(value==awayReleased||exiting)return;
        awayReleased=value;
        if(value) {
            reopenAfterAway=settingsWin!=null&&settingsWin.Visible;
            if(settingsWin!=null) {var old=settingsWin;settingsWin=null;old.Close();old.Dispose();}
            reconfiguring=true;
            try {foreach(var current in forms.ToArray()) {forms.Remove(current);current.Shutdown();current.Dispose();}form=null;}
            finally {reconfiguring=false;}
            Log.Write("Away mode: released wallpaper and Settings WebViews");
        } else {
            monitors.Refresh(true);StartForms(monitors.Current);ZOrderCoordinator.Ensure();
            if(reopenAfterAway) {reopenAfterAway=false;OpenSettings();}
            Log.Write("Away mode: desktop restored");
        }
    }
    void PostAwayState(string status)
    {
        if(settingsWin==null)return;
        int minutes=5;
        using(var key=Registry.CurrentUser.OpenSubKey(@"Control Panel\Desktop")) {int seconds;if(key!=null&&int.TryParse(key.GetValue("ScreenSaveTimeOut") as string,out seconds))minutes=Math.Max(1,Math.Min(120,seconds/60));}
        settingsWin.Post(Json.Stringify(Json.Obj("t","awayState","enabled",AwayRegistration.Enabled,"minutes",minutes,"busy",cacheBusy,"status",status??"","cached",File.Exists(Path.Combine(SaverPaths.Cache,"scenes.json")))));
    }
    bool HandleAwayMessage(Dictionary<string,object> message)
    {
        object type;message.TryGetValue("t",out type);string action=type as string;
        if(action=="awayStatus") {PostAwayState(null);return true;}
        if(action!="awayPreview"&&action!="awayEnable"&&action!="awayDisable"&&action!="awayRefresh"&&action!="awayLockImage")return false;
        HandleAwayAction(message,action);return true;
    }
    async void HandleAwayAction(Dictionary<string,object> message,string action)
    {
        if(cacheBusy)return;
        try {
            if(action=="awayDisable") {AwayRegistration.Configure(false,5);PostAwayState(I18n.T("awayDisabled"));return;}
            if(action=="awayEnable") {
                object value;double requested;
                if(!message.TryGetValue("minutes",out value)||value is string||value is bool)return;
                try {requested=Convert.ToDouble(value);}catch{return;}
                if(double.IsNaN(requested)||requested!=Math.Floor(requested)||requested<1||requested>120)return;
                if(!await PrepareSaverCache())return;
                AwayRegistration.Configure(true,(int)requested);PostAwayState(I18n.T("awayEnabled"));return;
            }
            if(!await PrepareSaverCache())return;
            if(action=="awayPreview") {
                Process.Start(new ProcessStartInfo(AwayRegistration.SaverExecutable,"/s") {WorkingDirectory=AppDomain.CurrentDomain.BaseDirectory});
            } else if(action=="awayLockImage") {
                string image=Directory.GetFiles(SaverPaths.Cache,"frame-*-0.png").OrderBy(x=>x).FirstOrDefault();
                if(image==null)throw new InvalidOperationException(I18n.T("awayNoScene"));
                string destination=Path.Combine(SaverPaths.Profile,"Deskland-lock.png");File.Copy(image,destination,true);
                await SetWindowsLockImage(destination);PostAwayState(I18n.T("awayLockDone"));
            }
        } catch(Exception ex) {Log.Write("Away mode: "+ex);PostAwayState(I18n.T("awayError","msg",ex.Message));}
    }
    internal async Task<bool> PrepareSaverCache()
    {
        if(cacheBusy||awayReleased||forms.Count==0)return false;
        cacheBusy=true;PostAwayState(I18n.T("awayPreparing"));
        var files=new HashSet<string>();
        try {
            Directory.CreateDirectory(SaverPaths.Cache);
            WallpaperForm[] snapshot=forms.ToArray();
            if(Array.Exists(snapshot,delegate(WallpaperForm f){return f.Status!="ready";}))throw new InvalidOperationException(I18n.T("errNotReady"));
            string prefix="frame-"+Guid.NewGuid().ToString("N");
            var scenes=new List<object>();
            var frames=new List<string>[snapshot.Length];
            for(int i=0;i<snapshot.Length;i++)frames[i]=new List<string>();
            var elapsed=Stopwatch.StartNew();
            for(int frame=0;frame<12;frame++) {
                for(int i=0;i<snapshot.Length;i++) {
                    if(awayReleased||snapshot[i].IsDisposed)throw new InvalidOperationException(I18n.T("errNotReady"));
                    string name=prefix+"-"+i+"-"+frame+".png";files.Add(name);
                    await snapshot[i].CaptureScaledScene(Path.Combine(SaverPaths.Cache,name));frames[i].Add(name);
                }
                int wait=(int)((frame+1)*1000.0/6-elapsed.Elapsed.TotalMilliseconds);
                if(wait>0)await Task.Delay(wait);
            }
            for(int i=0;i<snapshot.Length;i++) {
                Rectangle bounds=snapshot[i].Monitor.Bounds;
                scenes.Add(Json.Obj("device",snapshot[i].Monitor.DeviceName,"panorama",Settings.Mode==WallpaperMode.Panorama,"bounds",new[]{bounds.X,bounds.Y,bounds.Width,bounds.Height},"frames",frames[i]));
            }
            string manifest=Path.Combine(SaverPaths.Cache,"scenes.json"),temporary=manifest+".tmp";
            File.WriteAllText(temporary,Json.Stringify(Json.Obj("version",1,"fps",6,"scenes",scenes)),new UTF8Encoding(false));
            if(File.Exists(manifest))File.Replace(temporary,manifest,null);else File.Move(temporary,manifest);
            foreach(string old in Directory.GetFiles(SaverPaths.Cache,"frame-*.png")) if(!files.Contains(Path.GetFileName(old)))File.Delete(old);
            Log.Write("Away scene cache ready: "+snapshot.Length+" surfaces, 12 frames at 6 fps, max edge 960");
            return true;
        } finally {cacheBusy=false;PostAwayState(I18n.T("awayReady"));}
    }
    async Task SetWindowsLockImage(string image)
    {
        if(InstallProfile.Isolated)throw new InvalidOperationException("Isolated tests cannot change the Windows lock image.");
        string script=Path.GetFullPath(Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"..","..","tools","set-lock-image.ps1"));
        var start=new ProcessStartInfo(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System),@"WindowsPowerShell\v1.0\powershell.exe"),"-NoProfile -NonInteractive -ExecutionPolicy Bypass -File \""+script+"\" -ImagePath \""+image+"\"") {UseShellExecute=false,CreateNoWindow=true,RedirectStandardError=true,RedirectStandardOutput=true};
        using(var process=Process.Start(start)) {
            var error=process.StandardError.ReadToEndAsync();var output=process.StandardOutput.ReadToEndAsync();
            await Task.Run(delegate {if(!process.WaitForExit(20000)) {process.Kill();throw new TimeoutException("Windows lock-image operation timed out");}});
            string detail=await error;await output;
            if(process.ExitCode!=0)throw new InvalidOperationException(string.IsNullOrWhiteSpace(detail)?I18n.T("awayLockBlocked"):detail.Trim());
        }
    }
}
