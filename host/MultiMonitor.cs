// Deskland multi-monitor primitives.  This file deliberately contains no WebView2
// code so the topology and coordinate contract can be tested without a desktop.
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Text;
using System.Windows.Forms;
using System.Threading.Tasks;

public enum WallpaperMode
{
    Mirror,
    Independent,
    Panorama
}

public sealed class MonitorInfo
{
    public string Id;
    public string DeviceName;
    public Rectangle Bounds;
    public Rectangle WorkingArea;
    public float DpiX;
    public float DpiY;
    public float Scale;
    public int Orientation;
    public int RefreshRate;
    public bool Primary;

    public MonitorInfo Clone()
    {
        return (MonitorInfo)MemberwiseClone();
    }

    public override string ToString()
    {
        return string.Format("{0} {1} {2}x{3} @ {4:0}%", Id, DeviceName,
            Bounds.Width, Bounds.Height, Scale * 100f);
    }
}

public sealed class MonitorManager
{
    readonly object gate = new object();
    MonitorInfo[] monitors = new MonitorInfo[0];
    string signature = "";
    DateTime nextRefresh = DateTime.MinValue;
    readonly Func<MonitorInfo[]> discover;

    public event Action<MonitorInfo[]> TopologyChanged;
    public MonitorInfo[] Current { get { lock (gate) return Clone(monitors); } }
    public Rectangle VirtualBounds { get { return MonitorGeometry.Union(Current); } }

    public MonitorManager() : this(Discover) { }
    public MonitorManager(Func<MonitorInfo[]> discovery)
    {
        if (discovery == null) throw new ArgumentNullException("discovery");
        discover = discovery;
        Refresh(true);
    }

    public bool Refresh()
    {
        return Refresh(false);
    }

    // Display changes can produce a burst of WM_DISPLAYCHANGE messages.  A small
    // debounce keeps us from tearing down and rebuilding WebView2 repeatedly.
    public bool Refresh(bool immediate)
    {
        if (!immediate && DateTime.UtcNow < nextRefresh) return false;
        nextRefresh = DateTime.UtcNow.AddMilliseconds(250);
        MonitorInfo[] found = discover();
        string next = Signature(found);
        bool changed;
        lock (gate)
        {
            changed = next != signature;
            if (changed)
            {
                signature = next;
                monitors = found;
            }
        }
        if (changed && TopologyChanged != null) TopologyChanged(Clone(found));
        return changed;
    }

    public static MonitorInfo[] Discover()
    {
        Screen[] screens = Screen.AllScreens;
        var result = new List<MonitorInfo>(screens.Length);
        for (int i = 0; i < screens.Length; i++)
        {
            Screen s = screens[i];
            float dx = 96, dy = 96;
            int orientation = 0, rate = 0;
            MonitorNative.ReadMonitorMetrics(s, ref dx, ref dy, ref orientation, ref rate);
            string name = s.DeviceName ?? "DISPLAY" + (i + 1);
            string hardwareId = MonitorNative.Identity(name);
            string id = StableId(hardwareId ?? name, s.Bounds);
            result.Add(new MonitorInfo {
                Id = id, DeviceName = name, Bounds = s.Bounds, WorkingArea = s.WorkingArea,
                DpiX = dx, DpiY = dy, Scale = dx / 96f, Orientation = orientation,
                RefreshRate = rate, Primary = s.Primary
            });
        }
        result.Sort(delegate(MonitorInfo a, MonitorInfo b)
        {
            int c = a.Bounds.Left.CompareTo(b.Bounds.Left);
            return c != 0 ? c : a.Bounds.Top.CompareTo(b.Bounds.Top);
        });
        return result.ToArray();
    }

    public static string StableId(string deviceName, Rectangle bounds)
    {
        // Hardware instance path survives primary, resolution and origin changes.
        // A GDI device name is the explicit fallback for virtual/remote displays.
        string n = (deviceName ?? "DISPLAY").Trim().ToUpperInvariant();
        return n;
    }

    static string Signature(MonitorInfo[] list)
    {
        var b = new StringBuilder();
        for (int i = 0; i < list.Length; i++)
            b.Append(list[i].Id).Append('|').Append(list[i].Bounds).Append('|').Append(list[i].WorkingArea).Append('|')
             .Append(list[i].DpiX).Append(',').Append(list[i].Orientation).Append(',').Append(list[i].Primary).Append(',').Append(list[i].RefreshRate).Append(';');
        return b.ToString();
    }

    static MonitorInfo[] Clone(MonitorInfo[] source)
    {
        var r = new MonitorInfo[source.Length];
        for (int i = 0; i < source.Length; i++) r[i] = source[i].Clone();
        return r;
    }
}

public static class MonitorGeometry
{
    public static Rectangle Union(IList<MonitorInfo> list)
    {
        if (list == null || list.Count == 0) return Rectangle.Empty;
        Rectangle r = list[0].Bounds;
        for (int i = 1; i < list.Count; i++) r = Rectangle.Union(r, list[i].Bounds);
        return r;
    }

    public static Rectangle Union(MonitorInfo[] list)
    {
        return Union((IList<MonitorInfo>)list);
    }

    public static Rectangle Clip(Rectangle world, Rectangle viewport)
    {
        return Rectangle.Intersect(world, viewport);
    }

    public static Rectangle ToLocal(Rectangle screen, Rectangle monitor)
    {
        return new Rectangle(screen.Left - monitor.Left, screen.Top - monitor.Top, screen.Width, screen.Height);
    }

    public static Rectangle ToWorld(Rectangle local, Rectangle monitor)
    {
        return new Rectangle(local.Left + monitor.Left, local.Top + monitor.Top, local.Width, local.Height);
    }

    public static bool Touches(Rectangle a, Rectangle b)
    {
        return (a.Right == b.Left || a.Left == b.Right) &&
            a.Top < b.Bottom && a.Bottom > b.Top ||
            (a.Bottom == b.Top || a.Top == b.Bottom) &&
            a.Left < b.Right && a.Right > b.Left;
    }
}

public static class WallpaperLayout
{
    // Panorama uses one world-sized native surface. Windows clips the surface
    // to attached outputs, including holes, negative origins and staggered rows.
    // There is exactly one simulation (actors and random generator), so an
    // object crossing a monitor edge cannot be duplicated or diverge in time.
    public static MonitorInfo[] Plan(MonitorInfo[] connected, WallpaperMode mode)
    {
        if (mode != WallpaperMode.Panorama || connected.Length == 0) return connected;
        Rectangle b = MonitorGeometry.Union(connected);
        return new[] { new MonitorInfo { Id = "panorama", DeviceName = "Virtual desktop", Bounds = b,
            WorkingArea = b, DpiX = 96, DpiY = 96, Scale = 1, Primary = true } };
    }

    public static List<Rectangle> RouteIcons(IList<Rectangle> icons, Rectangle viewport)
    {
        var routed = new List<Rectangle>();
        for (int i = 0; i < icons.Count; i++)
            // Keep the whole rectangle at an edge: clipping here would change
            // the coastline's source shape on either side of the seam.
            if (icons[i].IntersectsWith(viewport)) routed.Add(MonitorGeometry.ToLocal(icons[i], viewport));
        return routed;
    }
    public static string IconMessage(DesktopIconSnapshot snapshot, Rectangle viewport)
    {
        var sb = new StringBuilder("{\"t\":\"icons\",\"vis\":");
        sb.Append(snapshot.Visible ? "true" : "false").Append(",\"r\":[");
        List<Rectangle> routed = RouteIcons(snapshot.Rectangles, viewport);
        for (int i = 0; i < routed.Count; i++)
        {
            Rectangle r = routed[i]; if (i > 0) sb.Append(',');
            sb.Append('[').Append(r.Left).Append(',').Append(r.Top).Append(',').Append(r.Right).Append(',').Append(r.Bottom).Append(']');
        }
        return sb.Append("]}").ToString();
    }
}

public sealed class RestartBudget
{
    readonly Queue<DateTime> failures = new Queue<DateTime>();
    public DateTime Due = DateTime.MinValue;
    public bool Blocked;
    public bool Failed(DateTime now)
    {
        while (failures.Count > 0 && (now - failures.Peek()).TotalMinutes >= 1) failures.Dequeue();
        failures.Enqueue(now);
        Blocked = failures.Count >= 5;
        Due = now.AddSeconds(Math.Min(16, Math.Pow(2, failures.Count - 1)));
        return !Blocked;
    }
    public bool CanStart(DateTime now) { return !Blocked && now >= Due; }
    public void Reset() { failures.Clear(); Blocked = false; Due = DateTime.MinValue; }
}

// One owner for the sibling order; individual forms never compete for the slot
// immediately after DefView. All calls occur on the WinForms UI thread.
static class ZOrderCoordinator
{
    sealed class Entry { public IntPtr Window, Parent, Anchor; }
    static readonly List<Entry> entries = new List<Entry>();
    public static void Register(IntPtr window, IntPtr parent, IntPtr anchor)
    {
        Remove(window); entries.Add(new Entry { Window = window, Parent = parent, Anchor = anchor });
        Ensure();
    }
    public static void Remove(IntPtr window) { entries.RemoveAll(delegate(Entry e) { return e.Window == window; }); }
    public static void Ensure()
    {
        var previous = new Dictionary<IntPtr, IntPtr>();
        foreach (Entry e in entries.ToArray())
        {
            if (!Native.IsWindow(e.Window) || !Native.IsWindow(e.Parent) || !Native.IsWindow(e.Anchor)) continue;
            IntPtr after;
            if (!previous.TryGetValue(e.Parent, out after)) after = e.Anchor;
            if (Native.GetWindow(after, 2) != e.Window)
                Native.SetWindowPos(e.Window, after, 0, 0, 0, 0, Native.SWP_NOMOVE | Native.SWP_NOSIZE | Native.SWP_NOACTIVATE);
            previous[e.Parent] = e.Window;
        }
    }
    public static bool Verify()
    {
        var previous = new Dictionary<IntPtr, IntPtr>();
        foreach (Entry e in entries)
        {
            IntPtr after;
            if (!previous.TryGetValue(e.Parent, out after)) after = e.Anchor;
            if (!Native.IsWindow(e.Window) || Native.GetWindow(after, 2) != e.Window) return false;
            previous[e.Parent] = e.Window;
        }
        return entries.Count > 0;
    }
}

public sealed class DesktopIconSnapshot
{
    public readonly List<Rectangle> Rectangles;
    public readonly bool Visible;
    public readonly int Version;

    public DesktopIconSnapshot(List<Rectangle> rectangles, bool visible, int version)
    {
        Rectangles = rectangles ?? new List<Rectangle>();
        Visible = visible;
        Version = version;
    }
}

public sealed class DesktopIconService : IDisposable
{
    readonly IconReader reader = new IconReader();
    readonly object gate = new object();
    readonly object readerGate = new object();
    Task<bool> pending;
    bool disposed;
    bool readFailure;
    DesktopIconSnapshot current = new DesktopIconSnapshot(new List<Rectangle>(), true, 0);
    string last = "";

    public DesktopIconSnapshot Current { get { lock (gate) return current; } }

    public void PollAsync()
    {
        if (disposed || (pending != null && !pending.IsCompleted)) return;
        pending = Task.Run(delegate {
            try { return Poll(); }
            catch (Exception ex) { Log.Write("Icon reader: " + ex.Message); return false; }
        });
    }

    public bool Poll()
    {
        lock (readerGate)
        {
        if (disposed) return false;
        bool visible;
        List<Native.RECT> raw = reader.Read(out visible);
        if (raw == null)
        {
            if (!readFailure) Log.Write("Desktop icons unavailable; retaining last valid geometry");
            readFailure = true; return false;
        }
        readFailure = false;
        var rects = new List<Rectangle>(raw.Count);
        var sb = new StringBuilder();
        sb.Append(visible ? '1' : '0').Append(':');
        for (int i = 0; i < raw.Count; i++)
        {
            Rectangle r = new Rectangle(raw[i].Left, raw[i].Top, raw[i].Right - raw[i].Left, raw[i].Bottom - raw[i].Top);
            rects.Add(r);
            sb.Append(r.Left).Append(',').Append(r.Top).Append(',').Append(r.Width).Append(',').Append(r.Height).Append(';');
        }
        string next = sb.ToString();
        if (next == last) return false;
        last = next;
        lock (gate) current = new DesktopIconSnapshot(rects, visible, current.Version + 1);
        return true;
        }
    }

    public void Dispose() { lock (readerGate) { disposed = true; reader.Dispose(); } }
}

static class MonitorNative
{
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct DISPLAY_DEVICE
    {
        public int cb;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string DeviceName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceString;
        public int StateFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceID;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceKey;
    }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern bool EnumDisplayDevices(string device, uint index, ref DISPLAY_DEVICE info, uint flags);
    public static string Identity(string name)
    {
        DISPLAY_DEVICE d = new DISPLAY_DEVICE(); d.cb = Marshal.SizeOf(d);
        if (EnumDisplayDevices(name, 0, ref d, 1) && !string.IsNullOrEmpty(d.DeviceID)) return d.DeviceID;
        return null;
    }
    [DllImport("shcore.dll")] static extern int GetDpiForMonitor(IntPtr hmonitor, int type, out uint x, out uint y);
    [DllImport("user32.dll")] static extern IntPtr MonitorFromRect(ref Native.RECT rect, uint flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern bool EnumDisplaySettings(string device, int mode, ref DEVMODE devmode);
    const int MDT_EFFECTIVE_DPI = 0, ENUM_CURRENT_SETTINGS = -1, MONITOR_DEFAULTTONEAREST = 2;

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct DEVMODE
    {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmDeviceName;
        public short dmSpecVersion, dmDriverVersion, dmSize, dmDriverExtra;
        public int dmFields, dmPositionX, dmPositionY, dmDisplayOrientation, dmDisplayFixedOutput;
        public short dmColor, dmDuplex, dmYResolution, dmTTOption, dmCollate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmFormName;
        public short dmLogPixels; public int dmBitsPerPel, dmPelsWidth, dmPelsHeight, dmDisplayFlags, dmDisplayFrequency;
        public int dmICMMethod, dmICMIntent, dmMediaType, dmDitherType, dmReserved1, dmReserved2, dmPanningWidth, dmPanningHeight;
    }

    public static void ReadMonitorMetrics(Screen screen, ref float dx, ref float dy, ref int orientation, ref int rate)
    {
        try
        {
            Native.RECT r = new Native.RECT { Left = screen.Bounds.Left, Top = screen.Bounds.Top, Right = screen.Bounds.Right, Bottom = screen.Bounds.Bottom };
            IntPtr hm = MonitorFromRect(ref r, MONITOR_DEFAULTTONEAREST);
            uint x, y;
            if (hm != IntPtr.Zero && GetDpiForMonitor(hm, MDT_EFFECTIVE_DPI, out x, out y) == 0) { dx = x; dy = y; }
        }
        catch { }
        try
        {
            DEVMODE d = new DEVMODE(); d.dmSize = (short)Marshal.SizeOf(typeof(DEVMODE));
            if (EnumDisplaySettings(screen.DeviceName, ENUM_CURRENT_SETTINGS, ref d)) { orientation = d.dmDisplayOrientation; rate = d.dmDisplayFrequency; }
        }
        catch { }
    }
}
