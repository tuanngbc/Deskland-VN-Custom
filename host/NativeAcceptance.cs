// Opt-in acceptance runner. No test path is enabled without an isolated profile.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Threading.Tasks;
using System.Windows.Forms;

static class NativeAcceptance
{
    static readonly List<object> results = new List<object>();
    static string output;
    static void Check(string name, bool passed, object detail)
    {
        results.Add(Json.Obj("test", name, "status", passed ? "NATIVE PASS" : "NATIVE FAIL", "detail", detail));
        Log.Write("Acceptance " + name + ": " + (passed ? "PASS " : "FAIL ") + Json.Stringify(detail));
    }
    static async Task Until(Func<bool> condition, string name, int timeout)
    {
        var deadline = Stopwatch.StartNew();
        while (!condition())
        {
            if (deadline.ElapsedMilliseconds > timeout) throw new TimeoutException(name);
            await Task.Delay(100);
        }
    }
    static bool Ready(HostContext h)
    {
        WallpaperForm[] list = h.Instances;
        int want = Settings.Mode == WallpaperMode.Panorama ? 1 : h.Connected.Length;
        if (list.Length != want) return false;
        foreach (WallpaperForm f in list) if (f.Status != "ready") return false;
        return list.Length > 0;
    }
    static async Task<object> Evaluate(WallpaperForm f, string script)
    {
        return Json.Parse(await f.Evaluate(script));
    }
    static void DesktopCapture(string path)
    {
        Rectangle b = SystemInformation.VirtualScreen;
        using (var bmp = new Bitmap(b.Width, b.Height))
        {
            using (Graphics g = Graphics.FromImage(bmp)) g.CopyFromScreen(b.Location, Point.Empty, b.Size);
            bmp.Save(path, ImageFormat.Png);
        }
    }
    static async Task UntilScript(SettingsForm form, string script, string name)
    {
        var elapsed=Stopwatch.StartNew();
        while(elapsed.ElapsedMilliseconds<15000)
        {
            if (Json.Parse(await form.Evaluate(script)).Equals(true)) return;
            await Task.Delay(100);
        }
        throw new TimeoutException(name);
    }
    static async Task ExerciseTheme(HostContext host, string theme, string name)
    {
        foreach (WallpaperForm f in host.Instances)
            host.OnSettingsMessage(Json.Stringify(Json.Obj("t", "theme", "id", theme, "monitor", f.MonitorId)));
        await Until(delegate { return Ready(host); }, name + " ready", 20000);
        WallpaperForm instance = host.Instances[0];
        // A ready message is sent after the WebGL shaders and field textures exist.
        // Wait for actual rendered frames instead of assuming navigation == pixels.
        await Until(delegate { return instance.Status == "ready"; }, "renderer handshake", 10000);
        double frames = Convert.ToDouble(await Evaluate(instance, "frameN"));
        var sample = Stopwatch.StartNew();
        while (sample.ElapsedMilliseconds < 1200) await Task.Delay(100);
        double next = Convert.ToDouble(await Evaluate(instance, "frameN"));
        Check(name + " frames", next > frames, Json.Obj("start", frames, "end", next));
        object metrics = await Evaluate(instance, "({fps:fpsEMA,frames:frameN,canvas:[cv.width,cv.height],icons:icons.length,gpu:GPU,mode:DeskWorld.contract?.mode,world:DeskWorld.contract?.world,viewport:DeskWorld.contract?.viewport,seed})");
        Check(name + " renderer", Convert.ToBoolean(await Evaluate(instance, "!!gl && !gl.isContextLost() && iconsSeen")), metrics);
        await instance.CapturePng(Path.Combine(output, name + "-renderer.png"));
    }

    static async Task PerformanceSample(HostContext host, string name)
    {
        WallpaperForm[] instances = host.Instances;
        var frames = new double[instances.Length];
        for (int i=0;i<instances.Length;i++) frames[i]=Convert.ToDouble(await Evaluate(instances[i],"frameN"));
        List<int> ids=instances[0].WebViewProcessIds();ids.Add(Process.GetCurrentProcess().Id);
        var cpu = new Dictionary<int,double>();
        foreach(int id in ids) try { using(Process p=Process.GetProcessById(id)) cpu[id]=p.TotalProcessorTime.TotalMilliseconds; } catch { }
        var elapsed=Stopwatch.StartNew();
        // A fixed sampling interval measures throughput; it is not a readiness delay.
        await Task.Delay(10000);
        double seconds=elapsed.Elapsed.TotalSeconds,totalCpu=0,totalMemory=0;
        var rates=new List<object>();
        for(int i=0;i<instances.Length;i++) rates.Add(Json.Obj("monitor",instances[i].MonitorId,"fps",(Convert.ToDouble(await Evaluate(instances[i],"frameN"))-frames[i])/seconds));
        foreach(int id in ids) try { using(Process p=Process.GetProcessById(id)) { totalMemory+=p.WorkingSet64; if(cpu.ContainsKey(id))totalCpu+=p.TotalProcessorTime.TotalMilliseconds-cpu[id]; } } catch { }
        bool valid = true; foreach (object rate in rates) { var rr = rate as Dictionary<string,object>; if (rr != null && Convert.ToDouble(rr["fps"]) < 0) valid = false; }
        results.Add(Json.Obj("test",name+" performance","status",valid ? "MEASURED" : "MEASURED / RENDERER RESET DURING SAMPLE", "detail",Json.Obj("seconds",seconds,"renderers",rates,"cpuPercentOfMachine",totalCpu/(seconds*1000*Environment.ProcessorCount)*100,"sumWorkingSetsMiB",totalMemory/1048576,"sharedPagesMayBeCountedMoreThanOnce",true)));
    }

    static async Task IndependentIsolation(HostContext host)
    {
        host.OnSettingsMessage("{\"t\":\"mode\",\"v\":\"independent\"}");
        await Until(delegate { return Ready(host); },"independent isolation ready",30000);
        if(host.Instances.Length<2) return;
        WallpaperForm a=host.Instances[0],b=host.Instances[1];
        host.OnSettingsMessage(Json.Stringify(Json.Obj("t","theme","monitor",a.MonitorId,"id","beach")));
        host.OnSettingsMessage(Json.Stringify(Json.Obj("t","theme","monitor",b.MonitorId,"id","meadow")));
        await Until(delegate { return Ready(host); },"independent distinct themes",30000);
        double before=Convert.ToDouble(await Evaluate(a,"frameN"));
        host.OnSettingsMessage(Json.Stringify(Json.Obj("t","theme","monitor",b.MonitorId,"id","beach")));
        await Until(delegate { return Ready(host); },"independent theme switch",30000);
        Check("independent theme change preserves other renderer",host.Instances[0]==a&&Convert.ToDouble(await Evaluate(a,"frameN"))>=before,true);
        host.OnSettingsMessage(Json.Stringify(Json.Obj("t","set","monitor",a.MonitorId,"theme","beach","key","sigma","value",60)));
        host.OnSettingsMessage(Json.Stringify(Json.Obj("t","set","monitor",b.MonitorId,"theme","beach","key","sigma","value",140)));
        var routingDeadline = Stopwatch.StartNew();
        while (routingDeadline.ElapsedMilliseconds < 5000 && (Convert.ToInt32(await Evaluate(a,"P.sigma")) != 60 || Convert.ToInt32(await Evaluate(b,"P.sigma")) != 140)) await Task.Delay(100);
        Check("independent parameter routing",Convert.ToInt32(await Evaluate(a,"P.sigma"))==60&&Convert.ToInt32(await Evaluate(b,"P.sigma"))==140,true);
        bool otherWasPaused = Convert.ToBoolean(await Evaluate(b,"paused"));
        host.OnSettingsMessage(Json.Stringify(Json.Obj("t","monitorPause","id",a.MonitorId,"v",true)));
        var pauseDeadline=Stopwatch.StartNew();
        while(pauseDeadline.ElapsedMilliseconds<5000 && !Convert.ToBoolean(await Evaluate(a,"paused"))) await Task.Delay(100);
        Check("independent per-monitor pause",Convert.ToBoolean(await Evaluate(a,"paused")) && Convert.ToBoolean(await Evaluate(b,"paused"))==otherWasPaused,
            Json.Obj("otherMonitorAutoPausedBefore",otherWasPaused));
        host.OnSettingsMessage(Json.Stringify(Json.Obj("t","monitorPause","id",a.MonitorId,"v",false)));
        host.OnSettingsMessage(Json.Stringify(Json.Obj("t","theme","monitor",b.MonitorId,"id","meadow")));
        await Until(delegate { return Ready(host); },"independent screenshot",30000);
        await a.CapturePng(Path.Combine(output,"independent-left-beach.png"));
        await b.CapturePng(Path.Combine(output,"independent-primary-meadow.png"));
        // A disposable maximized test window covers one real output. This tests
        // the native foreground pause policy without manipulating user windows.
        using(var cover=new Form())
        {
            cover.Text="Wallpaper acceptance cover";cover.StartPosition=FormStartPosition.Manual;
            cover.Bounds=b.Monitor.WorkingArea;cover.Show();cover.WindowState=FormWindowState.Maximized;cover.BringToFront();cover.Activate();
            Native.SetForegroundWindow(cover.Handle);
            if (Native.GetForegroundWindow()==cover.Handle)
            {
                await Task.Delay(700); // foreground policy polls every 500ms
                Check("fullscreen pause affects only covered monitor",!Convert.ToBoolean(await Evaluate(a,"paused"))&&Convert.ToBoolean(await Evaluate(b,"paused")),true);
            }
            else results.Add(Json.Obj("test","fullscreen pause affects only covered monitor","status","NOT TESTED / HARDWARE UNAVAILABLE","detail","Windows did not grant foreground activation to the disposable acceptance window."));
        }
        host.OpenSettings();
    }

    public static async void Run(HostContext host)
    {
        output = Path.Combine(Log.DataDir, "acceptance");
        Directory.CreateDirectory(output);
        try
        {
            await Until(delegate { return Ready(host) && host.SettingsWindow != null && host.SettingsWindow.IsReady; }, "initialization", 30000);
            await Until(delegate { foreach (WallpaperForm f in host.Instances) if (f.LastIcons.Length > 40) return true; return false; }, "desktop icons", 10000);
            var connected = new List<object>();
            foreach (MonitorInfo m in host.Connected) connected.Add(Json.Obj("id", m.Id, "bounds", m.Bounds.ToString(), "dpi", m.DpiX, "primary", m.Primary));
            File.WriteAllText(Path.Combine(output, "environment.json"), Json.Stringify(Json.Obj("windows", Environment.OSVersion.ToString(), "monitors", connected)), System.Text.Encoding.UTF8);
            Check("settings DOM", Json.Parse(await host.SettingsWindow.Evaluate("!!document.querySelector('#monitorMap button') && document.documentElement.classList.contains('ready')")).Equals(true), connected);
            if(Environment.GetEnvironmentVariable("DESKLAND_TEST_STAGE")=="restart")
            {
                Check("settings persisted across process restart",Settings.Mode==WallpaperMode.Panorama&&Settings.Theme=="meadow",Settings.Mode.ToString());
                Check("restart wallpaper ready",Ready(host),host.Instances.Length);
                return;
            }
            // Exercise the actual settings controls in the native WebView.
            await host.SettingsWindow.Evaluate("document.querySelector('[data-mode=independent]').click()");
            await Until(delegate { return Settings.Mode==WallpaperMode.Independent&&Ready(host); },"settings mode button",30000);
            Check("settings mode button",true,Settings.Mode.ToString());
            await IndependentIsolation(host);
            await UntilScript(host.SettingsWindow, "(() => {try {return frame.contentWindow.eval('frameN > 0 && !!gl')} catch {return false}})()", "settings live preview");
            Check("settings live preview WebGL",true,true);
            await host.SettingsWindow.CapturePng(Path.Combine(output, "settings.png"));
            foreach (string mode in new[] { "mirror", "independent", "panorama" })
            {
                host.OnSettingsMessage(Json.Stringify(Json.Obj("t", "mode", "v", mode)));
                await Until(delegate { return Ready(host); }, "mode " + mode, 30000);
                await ExerciseTheme(host, "beach", mode + "-beach");
                await ExerciseTheme(host, "meadow", mode + "-meadow");
                Check(mode + " native window count", host.Instances.Length == (mode == "panorama" ? 1 : host.Connected.Length), host.Instances.Length);
                Check(mode + " z-order below desktop icon layer", ZOrderCoordinator.Verify(), true);
                await PerformanceSample(host,mode);
            }
            host.OnSettingsMessage("{\"t\":\"pause\",\"v\":true}");
            WallpaperForm current = host.Instances[0];
            Check("manual pause", Convert.ToBoolean(await Evaluate(current, "paused")), true);
            double pausedFrame = Convert.ToDouble(await Evaluate(current, "frameN"));
            await Task.Delay(600);
            Check("pause stops frames", Convert.ToDouble(await Evaluate(current, "frameN")) == pausedFrame, pausedFrame);
            host.OnSettingsMessage("{\"t\":\"pause\",\"v\":false}");
            await Task.Delay(600);
            Check("resume frames", Convert.ToDouble(await Evaluate(current, "frameN")) > pausedFrame, true);
            host.OnSettingsMessage("{\"t\":\"apply\"}");
            string before = File.ReadAllText(Path.Combine(Log.DataDir, "settings.json"));
            Settings.Load();
            Check("settings save/load", Settings.Mode == WallpaperMode.Panorama && Settings.Theme == "meadow", before);
            // Capture actual desktop pixels (distinct from the renderer snapshot).
            host.SettingsWindow.Hide();
            await Task.Delay(300); // Windows hide animation must finish before desktop capture.
            DesktopCapture(Path.Combine(output, "desktop.png"));
            Native.RECT r; Native.GetWindowRect(current.Handle, out r);
            Check("wallpaper physical bounds", r.Left == current.Monitor.Bounds.Left && r.Top == current.Monitor.Bounds.Top && r.Right == current.Monitor.Bounds.Right && r.Bottom == current.Monitor.Bounds.Bottom,
                current.Monitor.Bounds.ToString());
            Check("wallpaper no taskbar", !current.ShowInTaskbar && (Native.GetWindowLong(current.Handle, Native.GWL_STYLE) & Native.WS_CHILD) != 0, true);
            int hostPid = Process.GetCurrentProcess().Id;
            double memory = Process.GetCurrentProcess().WorkingSet64 / (1024.0 * 1024.0);
            Check("host memory sample", true, Json.Obj("hostPid", hostPid, "hostWorkingSetMiB", memory, "webviewProcessesNotIncluded", true));
            // A real renderer process crash in the isolated profile, followed by
            // process-failure recovery. The desktop shell is never terminated.
            WallpaperForm previous = current;
            previous.CrashRendererForTest();
            await Until(delegate { return Ready(host) && host.Instances[0] != previous; }, "renderer crash recovery", 30000);
            Check("WebView2 renderer crash recovery", true, host.Instances[0].Status);
            WallpaperForm recovered=host.Instances[0];
            SettingsForm oldSettings=host.SettingsWindow;
            recovered.CrashBrowserForTest();
            await Until(delegate { return Ready(host) && host.Instances[0]!=recovered && host.SettingsWindow!=null && host.SettingsWindow!=oldSettings && host.SettingsWindow.IsReady; }, "browser and settings recovery", 30000);
            await UntilScript(host.SettingsWindow,"document.documentElement.classList.contains('ready')","recovered settings DOM");
            Check("WebView2 browser and Settings crash recovery",true,true);
            host.SettingsWindow.Show();
            await host.SettingsWindow.CapturePng(Path.Combine(output, "settings-after-recovery.png"));
            if (host.Connected.Length < 2)
                results.Add(Json.Obj("test", "M02-M10/M17 physical multi-monitor acceptance", "status", "NOT TESTED / HARDWARE UNAVAILABLE", "detail", "Only one real display connected; simulated topology tests do not count as native multi-monitor acceptance."));
        }
        catch (Exception ex)
        {
            Check("acceptance runner", false, ex.ToString());
        }
        finally
        {
            string resultName=Environment.GetEnvironmentVariable("DESKLAND_TEST_STAGE")=="restart" ? "restart-results.json" : "results.json";
            File.WriteAllText(Path.Combine(output, resultName), Json.Stringify(results), System.Text.Encoding.UTF8);
            host.FinishNativeAcceptance();
        }
    }
}
