// Native cached-scene screen saver. This binary does not load WebView2.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Web.Script.Serialization;
using System.Windows.Forms;

static class SaverPaths
{
    public static string Profile { get {
        string value=Environment.GetEnvironmentVariable("DESKLAND_PROFILE_DIR");
        return string.IsNullOrWhiteSpace(value) ? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),"DesklandPreview") : Path.GetFullPath(value);
    } }
    public static string Cache { get { return Path.Combine(Profile,"AwayScenes"); } }
    public static string MutexName { get {
        using(var hash=SHA256.Create()) return "Local\\DesklandPreview_Saver_"+BitConverter.ToString(hash.ComputeHash(Encoding.UTF8.GetBytes(Profile.ToUpperInvariant()))).Replace("-","").Substring(0,24);
    } }
    public static bool IsRunning()
    {
        try {
            using(Mutex mutex=Mutex.OpenExisting(MutexName)) {
                try { if(!mutex.WaitOne(0)) return true; }
                catch(AbandonedMutexException) { }
                mutex.ReleaseMutex(); return false;
            }
        } catch(WaitHandleCannotBeOpenedException) { return false; }
    }
    public static Rectangle Cover(Rectangle source, Rectangle destination)
    {
        double scale=Math.Max((double)destination.Width/source.Width,(double)destination.Height/source.Height);
        int w=(int)Math.Ceiling(source.Width*scale),h=(int)Math.Ceiling(source.Height*scale);
        return new Rectangle(destination.X+(destination.Width-w)/2,destination.Y+(destination.Height-h)/2,w,h);
    }
}

sealed class SaverScene : IDisposable
{
    public string Device;
    public Rectangle Bounds;
    public bool Panorama;
    public readonly List<Bitmap> Frames=new List<Bitmap>();
    public void Dispose() { foreach(var frame in Frames) frame.Dispose(); Frames.Clear(); }
    public static List<SaverScene> Load()
    {
        var scenes=new List<SaverScene>();
        Directory.CreateDirectory(SaverPaths.Cache);
        try {
            var serializer=new JavaScriptSerializer();
            var root=serializer.Deserialize<Dictionary<string,object>>(File.ReadAllText(Path.Combine(SaverPaths.Cache,"scenes.json")));
            foreach(object item in (System.Collections.IEnumerable)root["scenes"]) {
                var entry=(Dictionary<string,object>)item;
                var b=(System.Collections.IList)entry["bounds"];
                var scene=new SaverScene { Device=(string)entry["device"],Panorama=Convert.ToBoolean(entry["panorama"]),Bounds=new Rectangle(Convert.ToInt32(b[0]),Convert.ToInt32(b[1]),Convert.ToInt32(b[2]),Convert.ToInt32(b[3])) };
                scenes.Add(scene);
                foreach(object frame in (System.Collections.IEnumerable)entry["frames"]) {
                    string name=frame as string;
                    // Cache manifests cannot load arbitrary paths or unbounded images.
                    if(string.IsNullOrEmpty(name)||Path.GetFileName(name)!=name||scene.Frames.Count>=12) continue;
                    string path=Path.Combine(SaverPaths.Cache,name);
                    using(var original=new Bitmap(path)) {
                        if(original.Width>960||original.Height>960||original.Width<1||original.Height<1) throw new InvalidDataException("Scene exceeds cache limit");
                        scene.Frames.Add(new Bitmap(original));
                    }
                }
                if(scenes.Count>=4) break;
            }
        } catch(Exception ex) { File.AppendAllText(Path.Combine(SaverPaths.Profile,"saver.log"),ex.Message+Environment.NewLine); }
        return scenes;
    }
}

sealed class SaverForm : Form
{
    readonly SaverScene scene;
    readonly Rectangle monitor;
    readonly bool mini;
    readonly Stopwatch clock;
    readonly Font timeFont,dateFont;
    public int Draws;
    public SaverForm(SaverScene scene, Rectangle monitor, bool mini, Stopwatch clock)
    {
        this.scene=scene;this.monitor=monitor;this.mini=mini;this.clock=clock;
        FormBorderStyle=FormBorderStyle.None;ShowInTaskbar=false;StartPosition=FormStartPosition.Manual;
        Bounds=monitor;TopMost=!mini;BackColor=Color.FromArgb(18,29,36);DoubleBuffered=true;KeyPreview=true;
        timeFont=new Font("Segoe UI",mini?18:Math.Max(32,monitor.Height/16),FontStyle.Regular,GraphicsUnit.Pixel);
        dateFont=new Font("Segoe UI",mini?9:Math.Max(16,monitor.Height/58),FontStyle.Regular,GraphicsUnit.Pixel);
    }
    protected override void OnPaintBackground(PaintEventArgs e) { }
    protected override void OnPaint(PaintEventArgs e)
    {
        Draws++;
        Graphics g=e.Graphics;
        g.Clear(BackColor);
        if(scene!=null&&scene.Frames.Count>0) {
            Bitmap image=scene.Frames[(int)(clock.Elapsed.TotalSeconds*6)%scene.Frames.Count];
            g.CompositingMode=CompositingMode.SourceCopy;
            g.CompositingQuality=CompositingQuality.HighSpeed;
            g.SmoothingMode=SmoothingMode.HighSpeed;
            g.InterpolationMode=InterpolationMode.NearestNeighbor;
            if(scene.Panorama&&!mini) {
                float sx=(float)image.Width/scene.Bounds.Width,sy=(float)image.Height/scene.Bounds.Height;
                var src=new RectangleF((monitor.X-scene.Bounds.X)*sx,(monitor.Y-scene.Bounds.Y)*sy,monitor.Width*sx,monitor.Height*sy);
                g.DrawImage(image,ClientRectangle,src,GraphicsUnit.Pixel);
            } else g.DrawImage(image,SaverPaths.Cover(new Rectangle(Point.Empty,image.Size),ClientRectangle));
        }
        using(var shade=new SolidBrush(Color.FromArgb(36,0,0,0))) g.FillRectangle(shade,ClientRectangle);
        string time=DateTime.Now.ToString("HH:mm"),date=DateTime.Now.ToString("dddd, dd MMMM",CultureInfo.GetCultureInfo("vi-VN"));
        SizeF a=g.MeasureString(time,timeFont),b=g.MeasureString(date,dateFont);
        // Drift the overlay slowly; no stationary bright clock on OLED panels.
        float drift=mini?0:(float)Math.Sin(clock.Elapsed.TotalSeconds/65)*Math.Min(60,ClientSize.Width/20);
        float x=(ClientSize.Width-a.Width)/2+drift,y=ClientSize.Height*.72f-a.Height/2;
        g.TextRenderingHint=System.Drawing.Text.TextRenderingHint.AntiAliasGridFit;
        using(var shadow=new SolidBrush(Color.FromArgb(120,0,0,0))) g.DrawString(time,timeFont,shadow,x+2,y+3);
        g.DrawString(time,timeFont,Brushes.White,x,y);
        g.DrawString(date,dateFont,Brushes.White,(ClientSize.Width-b.Width)/2+drift,y+a.Height);
    }
    protected override void Dispose(bool disposing) { if(disposing) {timeFont.Dispose();dateFont.Dispose();} base.Dispose(disposing); }
}

sealed class SaverContext : ApplicationContext, IMessageFilter
{
    readonly SaverPowerWindow powerWindow=new SaverPowerWindow();
    readonly List<SaverForm> forms=new List<SaverForm>();
    readonly List<SaverScene> scenes;
    readonly System.Windows.Forms.Timer timer=new System.Windows.Forms.Timer();
    readonly Stopwatch clock=Stopwatch.StartNew();
    readonly Point initialMouse=Cursor.Position;
    readonly bool mini,test;
    readonly double initialCpu;
    readonly int testSeconds;
    bool displayOff;
    public SaverContext(IntPtr parent,bool test,int seconds)
    {
        this.test=test;this.testSeconds=seconds;mini=parent!=IntPtr.Zero;
        initialCpu=Process.GetCurrentProcess().TotalProcessorTime.TotalMilliseconds;
        scenes=SaverScene.Load();
        foreach(Screen screen in mini?new[] {Screen.PrimaryScreen}:Screen.AllScreens) {
            SaverScene scene=scenes.Find(delegate(SaverScene s) {return s.Panorama||s.Device==screen.DeviceName;}) ?? (scenes.Count>0?scenes[0]:null);
            Rectangle bounds=screen.Bounds;
            if(mini) { SaverNative.RECT rect; if(!SaverNative.GetClientRect(parent,out rect)) break; bounds=new Rectangle(0,0,Math.Max(1,rect.Right),Math.Max(1,rect.Bottom)); }
            var form=new SaverForm(scene,bounds,mini,clock);forms.Add(form);
            if(mini) { SaverNative.SetParent(form.Handle,parent);SaverNative.SetWindowLong(form.Handle,-16,0x50000000); }
            form.FormClosed+=delegate { if(!mini) ExitThread(); };
            form.Show();
            if(!mini) form.Activate();
        }
        if(test&&forms.Count==0) {
            var fallback=new SaverForm(null,new Rectangle(0,0,800,600),false,clock);forms.Add(fallback);fallback.Show();
        }
        Application.AddMessageFilter(this);
        if(!mini) Cursor.Hide();
        timer.Interval=1000/6;
        timer.Tick+=delegate {
            if(test&&clock.Elapsed.TotalSeconds>=testSeconds) { FinishTest();return; }
            if(mini&&!SaverNative.IsWindow(parent)) { ExitThread();return; }
            if(!mini&&!test&&clock.Elapsed.TotalSeconds>1.2&&Distance(Cursor.Position,initialMouse)>8) { ExitThread();return; }
            if(!displayOff) foreach(var form in forms) form.Invalidate();
        };
        timer.Start();
        powerWindow.Changed+=delegate(bool off) {displayOff=off;timer.Interval=off?1000:1000/6;};
        Microsoft.Win32.SystemEvents.PowerModeChanged+=OnPower;
        Microsoft.Win32.SystemEvents.SessionSwitch+=OnSession;
    }
    static int Distance(Point a,Point b) { return Math.Abs(a.X-b.X)+Math.Abs(a.Y-b.Y); }
    void OnPower(object sender,Microsoft.Win32.PowerModeChangedEventArgs e) { displayOff=e.Mode==Microsoft.Win32.PowerModes.Suspend; }
    void OnSession(object sender,Microsoft.Win32.SessionSwitchEventArgs e) { if(e.Reason==Microsoft.Win32.SessionSwitchReason.SessionLogoff) ExitThread(); }
    public bool PreFilterMessage(ref Message m)
    {
        if(mini) return false;
        if(m.Msg==0x100&&((Keys)m.WParam==Keys.Escape||clock.Elapsed.TotalSeconds>1.2)) { ExitThread();return true; }
        if(clock.Elapsed.TotalSeconds>1.2&&(m.Msg==0x201||m.Msg==0x204||m.Msg==0x207)) { ExitThread();return true; }
        return false;
    }
    void FinishTest()
    {
        string dir=Path.Combine(SaverPaths.Profile,"saver-test");Directory.CreateDirectory(dir);
        int draws=0;
        using(var process=Process.GetCurrentProcess()) {
            var result=new Dictionary<string,object> { {"seconds",clock.Elapsed.TotalSeconds},{"draws",forms.Sum(delegate(SaverForm form){return form.Draws;})},{"surfaces",forms.Count},{"cpuPercentOfMachine",(process.TotalProcessorTime.TotalMilliseconds-initialCpu)/(clock.Elapsed.TotalMilliseconds*Environment.ProcessorCount)*100},{"privateMiB",process.PrivateMemorySize64/1048576.0},{"workingSetMiB",process.WorkingSet64/1048576.0},{"cachedScenes",scenes.Count},{"webView2",false} };
            File.WriteAllText(Path.Combine(dir,"results.json"),new JavaScriptSerializer().Serialize(result));
        }
        for(int i=0;i<forms.Count;i++) {
            var form=forms[i];draws+=form.Draws;
            using(var bitmap=new Bitmap(form.ClientSize.Width,form.ClientSize.Height)) { form.DrawToBitmap(bitmap,form.ClientRectangle);bitmap.Save(Path.Combine(dir,"screen-"+i+".png"),ImageFormat.Png); }
        }
        ExitThread();
    }
    protected override void ExitThreadCore()
    {
        timer.Stop();timer.Dispose();powerWindow.Dispose();Application.RemoveMessageFilter(this);
        Microsoft.Win32.SystemEvents.PowerModeChanged-=OnPower;Microsoft.Win32.SystemEvents.SessionSwitch-=OnSession;
        foreach(var form in forms.ToArray()) { form.FormClosed-=delegate { };form.Dispose(); }
        foreach(var scene in scenes) scene.Dispose();
        if(!mini) Cursor.Show();base.ExitThreadCore();
    }
}

static class SaverNative
{
    [DllImport("user32.dll")] internal static extern IntPtr RegisterPowerSettingNotification(IntPtr window,ref Guid setting,uint flags);
    [DllImport("user32.dll")] internal static extern bool UnregisterPowerSettingNotification(IntPtr handle);
    [DllImport("user32.dll",SetLastError=true)] internal static extern bool SystemParametersInfo(uint action,int param,ref int value,uint flags);
    [StructLayout(LayoutKind.Sequential)] internal struct RECT {public int Left,Top,Right,Bottom;}
    [DllImport("user32.dll")] internal static extern bool GetClientRect(IntPtr handle,out RECT rect);
    [DllImport("user32.dll")] internal static extern IntPtr SetParent(IntPtr child,IntPtr parent);
    [DllImport("user32.dll")] internal static extern int SetWindowLong(IntPtr handle,int index,int value);
    [DllImport("user32.dll")] internal static extern bool IsWindow(IntPtr handle);
    [DllImport("user32.dll")] internal static extern bool SetProcessDpiAwarenessContext(IntPtr value);
}

sealed class SaverPowerWindow : NativeWindow,IDisposable
{
    IntPtr notification;
    public event Action<bool> Changed;
    public SaverPowerWindow()
    {
        CreateHandle(new CreateParams {Parent=new IntPtr(-3)});
        Guid display=new Guid("6FE69556-704A-47A0-8F24-C28D936FDA47");
        notification=SaverNative.RegisterPowerSettingNotification(Handle,ref display,0);
    }
    protected override void WndProc(ref Message message)
    {
        if(message.Msg==0x218&&message.WParam.ToInt64()==0x8013&&message.LParam!=IntPtr.Zero&&Marshal.ReadInt32(message.LParam,16)>=4) {
            int value=Marshal.ReadInt32(message.LParam,20);
            if(Changed!=null)Changed(value==0);
        }
        base.WndProc(ref message);
    }
    public void Dispose() {if(notification!=IntPtr.Zero)SaverNative.UnregisterPowerSettingNotification(notification);notification=IntPtr.Zero;if(Handle!=IntPtr.Zero)DestroyHandle();}
}

static class DesklandSaverProgram
{
    [STAThread] static void Main(string[] args)
    {
        Directory.CreateDirectory(SaverPaths.Profile);
        string option=args.Length==0?"/c":args[0].ToLowerInvariant();
        File.AppendAllText(Path.Combine(SaverPaths.Profile,"saver.log"),"start "+option+Environment.NewLine);
        if(option.StartsWith("/c")) { Process.Start(new ProcessStartInfo(Path.Combine(AppDomain.CurrentDomain.BaseDirectory,"Deskland.exe"),"--settings"));return; }
        IntPtr parent=IntPtr.Zero;
        if(option.StartsWith("/p")) { long value;string text=option.Length>3?option.Substring(3):(args.Length>1?args[1]:"0");if(!long.TryParse(text,out value)||value==0)return;parent=new IntPtr(value); }
        else if(option!="/s"&&option!="--test") return;
        bool test=option=="--test";
        if(test&&string.IsNullOrWhiteSpace(Environment.GetEnvironmentVariable("DESKLAND_PROFILE_DIR"))) return;
        SaverNative.SetProcessDpiAwarenessContext(new IntPtr(-4));
        Application.EnableVisualStyles();Application.SetCompatibleTextRenderingDefault(false);
        bool created;
        // Miniature Control Panel previews must not stop the desktop wallpaper.
        using(var active=new Mutex(parent==IntPtr.Zero,SaverPaths.MutexName,out created)) {
            if(parent==IntPtr.Zero&&!created) { try {if(!active.WaitOne(0))return;}catch(AbandonedMutexException){} }
            try { Application.Run(new SaverContext(parent,test,10)); }
            finally {if(parent==IntPtr.Zero)active.ReleaseMutex();}
        }
    }
}
