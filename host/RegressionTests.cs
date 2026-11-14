using System;
using System.Collections.Generic;
using System.Drawing;
using System.IO;

static class RegressionTests
{
    static int passed, failed;
    static void Test(string name, Action test)
    {
        try { test(); passed++; Console.WriteLine("AUTOMATED PASS " + name); }
        catch (Exception e) { failed++; Console.WriteLine("AUTOMATED FAIL " + name + ": " + e.Message); }
    }
    static void Assert(bool condition, string reason) { if (!condition) throw new Exception(reason); }
    static MonitorInfo Monitor(string id, int x, int y, int w, int h)
    {
        return new MonitorInfo { Id = id, DeviceName = id, Bounds = new Rectangle(x, y, w, h), WorkingArea = new Rectangle(x,y,w,h-40), DpiX = 96, DpiY = 96, Scale = 1 };
    }
    static int Main(string[] args)
    {
        if (!InstallProfile.Isolated) throw new Exception("Tests require an isolated profile");
        string pageDir = Path.GetFullPath(Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "../../prototype"));
        Test("negative-origin-staggered-world", delegate {
            var ms = new[] { Monitor("portrait",-1080,-685,1080,1920), Monitor("wide",0,0,2560,1080) };
            Assert(MonitorGeometry.Union(ms) == new Rectangle(-1080,-685,3640,1920), "wrong world bounds");
        });
        Test("panorama-is-one-simulation", delegate {
            var ms = new[] { Monitor("a",-1920,0,1920,1080), Monitor("b",0,0,2560,1440), Monitor("c",300,1440,1080,1920) };
            var plan = WallpaperLayout.Plan(ms, WallpaperMode.Panorama);
            Assert(plan.Length == 1 && plan[0].Id == "panorama", "duplicate simulations");
            Assert(plan[0].Bounds == MonitorGeometry.Union(ms), "world not union");
        });
        Test("mirror-keeps-aspect-ratios", delegate {
            var ms = new[] { Monitor("a",0,0,1080,1920), Monitor("b",1080,0,2560,1080) };
            var p = WallpaperLayout.Plan(ms, WallpaperMode.Mirror);
            Assert(p.Length == 2 && p[0].Bounds.Size != p[1].Bounds.Size, "stretched primary");
        });
        Test("independent-has-one-slot-per-monitor", delegate {
            var ms = new[] { Monitor("a",0,0,1920,1080), Monitor("b",1920,0,1280,720), Monitor("c",0,-1200,1920,1200), Monitor("d",-1440,0,1440,2560) };
            Assert(WallpaperLayout.Plan(ms, WallpaperMode.Independent).Length == 4, "hardcoded count");
        });
        Test("empty-topology", delegate { Assert(WallpaperLayout.Plan(new MonitorInfo[0], WallpaperMode.Panorama).Length == 0 && MonitorGeometry.Union(new MonitorInfo[0]).IsEmpty, "empty desktop"); });
        Test("topology-events-no-duplicate-reconfigure", delegate {
            var topology=new[] { Monitor("a",0,0,1920,1080) };
            var manager=new MonitorManager(delegate { return topology; });int events=0;
            manager.TopologyChanged+=delegate(MonitorInfo[] update) { events++; };
            manager.Refresh(true);manager.Refresh(true);Assert(events==0,"duplicate reconfigure");
            topology=new[] { Monitor("a",0,0,1920,1080), Monitor("b",-1080,-685,1080,1920) };
            manager.Refresh(true);Assert(events==1&&manager.Current.Length==2,"hotplug not discovered");
            manager.Refresh(true);Assert(events==1,"topology burst recreated surfaces");
            topology=new[] { topology[1] };manager.Refresh(true);Assert(events==2&&manager.Current.Length==1,"unplug not discovered");
        });
        Test("topology-dpi-primary-resolution-change-retains-id", delegate {
            var topology=new[] { Monitor("stable",0,0,1920,1080) };var manager=new MonitorManager(delegate { return topology; });int events=0;
            manager.TopologyChanged+=delegate(MonitorInfo[] update){events++;};
            topology[0]=Monitor("stable",-2560,-720,2560,1440);topology[0].DpiX=144;topology[0].Primary=true;
            manager.Refresh(true);Assert(events==1&&manager.Current[0].Id=="stable"&&manager.Current[0].DpiX==144,"metrics not updated");
            var external=manager.Current;external[0].Id="mutated";Assert(manager.Current[0].Id=="stable","topology is mutable externally");
        });
        Test("physical-gap-remains-gap", delegate {
            Rectangle a = new Rectangle(0,0,1920,1080), b = new Rectangle(2200,0,1920,1080);
            Assert(!MonitorGeometry.Touches(a,b), "collapsed physical gap");
            Assert(MonitorGeometry.Clip(new Rectangle(1950,50,50,50),a).IsEmpty, "gap visible on output");
        });
        Test("edge-adjacency-not-corner-adjacency", delegate {
            Rectangle a=new Rectangle(0,0,100,100);
            Assert(MonitorGeometry.Touches(a,new Rectangle(100,20,100,100)), "edge missing");
            Assert(!MonitorGeometry.Touches(a,new Rectangle(100,100,100,100)), "corners touch incorrectly");
        });
        Test("world-local-roundtrip-1000-rectangles", delegate {
            var random = new Random(6021);
            for (int i=0;i<1000;i++) {
                var viewport = new Rectangle(random.Next(-8000,8000),random.Next(-8000,8000),random.Next(300,8000),random.Next(300,8000));
                var icon = new Rectangle(random.Next(-8000,8000),random.Next(-8000,8000),80,90);
                Assert(MonitorGeometry.ToWorld(MonitorGeometry.ToLocal(icon,viewport),viewport)==icon,"round trip");
            }
        });
        Test("icon-routing-no-cross-monitor-leak", delegate {
            var icons = new List<Rectangle> { new Rectangle(-900,100,80,90),new Rectangle(300,100,80,90) };
            var left=WallpaperLayout.RouteIcons(icons,new Rectangle(-1920,0,1920,1080));
            var right=WallpaperLayout.RouteIcons(icons,new Rectangle(0,0,1920,1080));
            Assert(left.Count==1&&right.Count==1&&left[0].X==1020&&right[0].X==300,"misrouted icons");
        });
        Test("boundary-icon-preserves-full-shape", delegate {
            var icons = new List<Rectangle> { new Rectangle(-20,50,80,90) };
            var a=WallpaperLayout.RouteIcons(icons,new Rectangle(-1920,0,1920,1080));
            var b=WallpaperLayout.RouteIcons(icons,new Rectangle(0,0,1920,1080));
            Assert(a.Count==1&&b.Count==1&&a[0].Width==80&&b[0].X==-20,"icon was clipped/distorted");
        });
        Test("panorama-icons-have-one-world-origin", delegate {
            var snapshot=new DesktopIconSnapshot(new List<Rectangle>{new Rectangle(-1000,-600,80,90),new Rectangle(50,20,80,90)},true,1);
            var data=(Dictionary<string,object>)Json.Parse(WallpaperLayout.IconMessage(snapshot,new Rectangle(-1080,-685,3640,1920)));
            var r=(object[])data["r"];var first=(object[])r[0];
            Assert(r.Length==2&&Convert.ToInt32(first[0])==80&&Convert.ToInt32(first[1])==85,"wrong world origin");
        });
        Test("hidden-icons-empty-safe-message", delegate { var d=(Dictionary<string,object>)Json.Parse(WallpaperLayout.IconMessage(new DesktopIconSnapshot(new List<Rectangle>(),false,1),new Rectangle(0,0,1920,1080))); Assert(!(bool)d["vis"]&&((object[])d["r"]).Length==0,"hidden state"); });
        Test("monitor-identity-survives-primary-resolution-origin", delegate { Assert(MonitorManager.StableId("hardware-instance",new Rectangle(-100,0,100,200))==MonitorManager.StableId("hardware-instance",new Rectangle(0,0,3840,2160)),"unstable ID"); });
        Test("restart-budget-bounded-and-manually-resettable", delegate {
            var b=new RestartBudget();DateTime t=new DateTime(2026,1,1);
            for(int i=0;i<5;i++) b.Failed(t.AddSeconds(i));
            Assert(b.Blocked&&!b.CanStart(t.AddHours(1)),"infinite crash loop"); b.Reset(); Assert(b.CanStart(t),"reload cannot recover");
        });
        Test("restart-budget-does-not-penalize-separated-failures", delegate { var b=new RestartBudget();DateTime t=new DateTime(2026,1,1);for(int i=0;i<20;i++) b.Failed(t.AddMinutes(i*2));Assert(!b.Blocked,"old failures retained"); });
        Test("restart-budget-exponential-backoff", delegate { var b=new RestartBudget();DateTime t=new DateTime(2026,1,1);b.Failed(t);Assert(!b.CanStart(t)&&b.CanStart(t.AddSeconds(1)),"backoff absent");b.Failed(t.AddSeconds(1));Assert(b.Due==t.AddSeconds(3),"backoff not increasing"); });
        Directory.CreateDirectory(Log.DataDir);
        string settings=Path.Combine(Log.DataDir,"settings.json");
        Test("settings-v1-lossless-migration", delegate {
            File.WriteAllText(settings,"{\"version\":1,\"theme\":\"meadow\",\"lang\":\"en\",\"params\":{\"meadow\":{\"sigma\":99,\"seed\":7}},\"ownerExtension\":42}");
            Settings.Load();Settings.Save();var root=(Dictionary<string,object>)Json.Parse(File.ReadAllText(settings));
            Assert(Settings.Theme=="meadow"&&Settings.Mode==WallpaperMode.Mirror&&Convert.ToInt32(Settings.For("meadow")["sigma"])==99,"migration lost settings");
            Assert(Convert.ToInt32(root["version"])==3&&Convert.ToInt32(root["ownerExtension"])==42,"schema/unknown fields");
        });
        Test("settings-independent-params-isolated", delegate {
            Settings.Mode=WallpaperMode.Independent;
            Settings.ForMonitor("a","meadow")["sigma"]=50;
            Settings.ForMonitor("b","meadow")["sigma"]=150;
            Assert(Convert.ToInt32(Settings.ForMonitor("a","meadow")["sigma"])==50&&Convert.ToInt32(Settings.For("meadow")["sigma"])==99,"params leaked");
        });
        Test("settings-disconnected-monitor-retained", delegate {
            Settings.MonitorThemes["unplugged"]="meadow";Settings.MonitorPaused["unplugged"]=true;Settings.Save();Settings.Load();
            Assert(Settings.MonitorThemes["unplugged"]=="meadow"&&Settings.MonitorPaused["unplugged"],"disconnect lost config");
        });
        Test("settings-independent-fallback", delegate { Assert(Settings.ThemeFor(Monitor("unknown",0,0,100,100))==Settings.Theme,"missing fallback"); });
        Test("settings-common-mode-ignores-independent-theme", delegate {Settings.Mode=WallpaperMode.Panorama;Settings.Theme="beach";Assert(Settings.ThemeFor(Monitor("unplugged",0,0,100,100))=="beach","independent leaked into panorama");});
        Test("settings-atomic-backup", delegate { Settings.Save();Assert(File.Exists(settings+".bak")&&!File.Exists(settings+".tmp"),"backup/temp"); });
        Test("settings-restore-last-working", delegate { Settings.MarkWorking();Settings.Theme="meadow";Settings.Save();Assert(Settings.RestoreWorking()&&Settings.Theme=="beach","working state lost"); });
        Test("settings-corrupt-file-recovers-backup", delegate { Settings.Save();Settings.Save();File.WriteAllText(settings,"{broken");Settings.Load();Assert(Settings.Theme=="beach","backup recovery"); });
        Test("theme-locked-never-loadable", delegate {
            string fixture = Path.Combine(Log.DataDir,"locked-theme"); Directory.CreateDirectory(fixture);
            File.WriteAllText(Path.Combine(fixture,"locked.html"),"<!doctype html>");
            File.WriteAllText(Path.Combine(fixture,"themes.json"),"{\"themes\":[{\"id\":\"locked\",\"status\":\"locked\",\"product\":\"free\",\"page\":\"locked.html\"}]}");
            Assert(!Themes.Allowed(fixture,"locked",delegate(string product){return true;}),"locked renderer accepted");
        });
        Test("vietnamese-theme-pages-available", delegate { foreach(string id in new[] {"meadow","beach","beach-turquoise","lake","sky","ocean","rainforest","mocchau","tet-sakura","cat-hill","stadium","aquarium","commute","summer-porch","cats","boxing-ring"}) Assert(Themes.Allowed(pageDir,id,null),"theme unavailable: "+id); });
        Test("vietnamese-language-and-tray", delegate { Assert(I18n.FromCulture(new System.Globalization.CultureInfo("vi-VN"))=="vi","culture mapping"); I18n.Load(pageDir,"vi"); Assert(I18n.Code=="vi"&&I18n.T("traySettings").Contains("Cài đặt"),"Vietnamese tray missing"); });
        Test("theme-ready-has-real-page", delegate { Assert(Themes.Allowed(pageDir,"beach",null)&&Themes.Allowed(pageDir,"meadow",null),"ready missing"); });
        Test("theme-unknown-rejected", delegate {Assert(Themes.PageFor(pageDir,"../../anything")==null,"unknown page");});
        Test("parameter-range-validation", delegate {Assert(Themes.ValidParameter(pageDir,"beach","sigma",85)&&!Themes.ValidParameter(pageDir,"beach","sigma",-1)&&!Themes.ValidParameter(pageDir,"beach","sigma",10000),"range validation");});
        Test("parameter-invalid-message-rejected", delegate {Assert(!Themes.ValidParameter(pageDir,"beach","unknown",1)&&!Themes.ValidParameter(pageDir,"beach","sigma","oops")&&!Themes.ValidParameter(pageDir,"beach","seed",double.NaN),"unsafe message");});
        Test("parameter-enum-validation", delegate {Assert(Themes.ValidParameter(pageDir,"meadow","showCat",1)&&!Themes.ValidParameter(pageDir,"meadow","showCat",1.5)&&!Themes.ValidParameter(pageDir,"meadow","showCat",2),"invalid option");});
        Test("settings-future-schema-not-downgraded", delegate {string data="{\"version\":99,\"theme\":\"beach\",\"future\":true}";File.WriteAllText(settings,data);Settings.Load();Settings.Save();Assert(File.ReadAllText(settings)==data,"newer schema overwritten");});
        Console.WriteLine("TOTAL: " + passed + " PASS / " + failed + " FAIL");
        return failed==0?0:1;
    }
}
