//! A small native taskbar monitor. Sampling never blocks the wallpaper/UI thread.
mod stats;
mod taskbar;

use std::sync::{mpsc, Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::HWND;

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub enabled: bool,
    pub interval: u64,
}
impl Default for Options {
    fn default() -> Self { Self { enabled: true, interval: 1 } }
}
impl Options {
    pub fn from_json(value: &serde_json::Value) -> Self {
        Self {
            enabled: value["enabled"].as_bool().unwrap_or(true),
            interval: match value["interval"].as_u64() { Some(2) => 2, Some(5) => 5, _ => 1 },
        }
    }
    pub fn json(&self) -> serde_json::Value { serde_json::json!({"enabled": self.enabled, "interval": self.interval}) }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Snapshot {
    pub(crate) cpu: Option<f64>,
    pub(crate) ram: Option<f64>,
    pub(crate) gpu: Option<f64>,
    pub(crate) down: Option<f64>,
    pub(crate) up: Option<f64>,
    pub(crate) ram_used: u64,
    pub(crate) ram_total: u64,
    sequence: u64,
    sample_ms: f64,
}
impl Snapshot {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({"cpu":self.cpu,"ram":self.ram,"gpu":self.gpu,"downloadBytesPerSecond":self.down,
            "uploadBytesPerSecond":self.up,"ramUsed":self.ram_used,"ramTotal":self.ram_total,"sequence":self.sequence,"sampleMs":self.sample_ms})
    }
}

/// Weak ownership allows the counters to close when the last visible
/// consumer goes away. Only background worker threads call sample().
struct FeedState {
    sampler: stats::Sampler,
    recent: Option<(Instant, u8, Snapshot)>,
    sequence: u64,
}
#[derive(Clone)]
pub(crate) struct Feed(Arc<Mutex<Option<FeedState>>>);
static LIVE_FEED: Mutex<Weak<Mutex<Option<FeedState>>>> = Mutex::new(Weak::new());
impl Feed {
    pub(crate) fn shared()->Self {
        // Creating a lease on the UI thread does not open Windows counters.
        let mut weak=LIVE_FEED.lock().unwrap_or_else(|e|e.into_inner());
        if let Some(inner)=weak.upgrade(){return Self(inner);}
        let inner=Arc::new(Mutex::new(None));
        *weak=Arc::downgrade(&inner);
        Self(inner)
    }
    pub(crate) fn sample(&self, mask:u8, interval:u64)->Snapshot {
        let Ok(mut guard)=self.0.lock()else{return Snapshot::default();};
        let state=guard.get_or_insert_with(||FeedState{sampler:stats::Sampler::new(),recent:None,sequence:0});
        if let Some((at,sampled,value))=&state.recent{
            let recently=at.elapsed()<Duration::from_millis(interval.saturating_mul(1000).saturating_sub(100));
            if recently&&mask&!*sampled==0{return value.clone();}
            // Reuse the existing counter baseline if another consumer still
            // asks for fields this one does not show.
        }
        let mut value=state.sampler.sample_selected(mask);
        state.sequence=state.sequence.wrapping_add(1);
        value.sequence=state.sequence;
        state.recent=Some((Instant::now(),mask,value.clone()));
        value
    }
}
#[derive(Clone, Default)]
struct Shared {
    snapshot: Snapshot,
    dock: Option<taskbar::Dock>,
}
enum Command { Suspend(bool), Interval(u64), Stop }

pub struct Monitor {
    window: taskbar::Window,
    shared: Arc<Mutex<Shared>>,
    commands: mpsc::Sender<Command>,
    suspended: bool,
}
impl Monitor {
    pub fn new(owner: HWND, options: &Options) -> Result<Self, String> {
        let window = taskbar::Window::new(owner)?;
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (commands, receiver) = mpsc::channel();
        let worker_shared = shared.clone();
        let interval = options.interval;
        std::thread::Builder::new().name("taskbar-stats".into()).spawn(move || {
            let com = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED) };
            let mut probe = taskbar::Probe::new();
            let mut sampler = Some(Feed::shared());
            let mut interval = interval;
            let mut suspended = false;
            let mut next_dock = Instant::now();
            loop {
                if !suspended {
                    if let Some(sampler) = sampler.as_mut() {
                        let snapshot = sampler.sample(31,interval);
                        if let Ok(mut state) = worker_shared.lock() { state.snapshot = snapshot; }
                    }
                    if Instant::now() >= next_dock {
                        // Explorer can briefly omit its WinUI tray during an
                        // overflow animation. Keep the last valid dock until
                        // a new one is available; HWND visibility is checked live.
                        if let Some(dock) = probe.read() {
                            if let Ok(mut state) = worker_shared.lock() { state.dock = Some(dock); }
                        }
                        next_dock = Instant::now() + Duration::from_secs(5);
                    }
                }
                let result = if suspended { receiver.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected) }
                    else { receiver.recv_timeout(Duration::from_secs(interval)) };
                match result {
                    Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Ok(Command::Interval(value)) => interval = value,
                    Ok(Command::Suspend(value)) => {
                        suspended = value;
                        // Close PDH handles while locked. Re-prime counters on
                        // resume so idle time is never reported as a new spike.
                        sampler = if suspended { None } else { Some(Feed::shared()) };
                        next_dock = Instant::now();
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
            drop(sampler);
            drop(probe);
            if com.is_ok() { unsafe { windows::Win32::System::Com::CoUninitialize(); } }
        }).map_err(|error| error.to_string())?;
        Ok(Self { window, shared, commands, suspended: false })
    }

    pub fn set_interval(&self, interval: u64) { let _ = self.commands.send(Command::Interval(interval)); }
    pub fn tick(&mut self, suspended: bool) {
        if self.suspended != suspended {
            self.suspended = suspended;
            let _ = self.commands.send(Command::Suspend(suspended));
        }
        let state = self.shared.lock().ok().map(|value| value.clone());
        if let Some(state) = state { self.window.update(&state.snapshot, state.dock.as_ref(), suspended); }
    }
}
impl Drop for Monitor {
    fn drop(&mut self) { let _ = self.commands.send(Command::Stop); }
}

/// Read-only diagnostics, including a native-rendered preview beside the JSON.
pub fn diagnostics(path: &std::path::Path) -> std::io::Result<()> {
    unsafe {
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let com = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED) };
    let mut probe = taskbar::Probe::new();
    let dock = probe.read();
    let mut sampler = stats::Sampler::new();
    let _ = sampler.sample();
    std::thread::sleep(Duration::from_secs(1));
    let snapshot = sampler.sample();
    let report = serde_json::json!({"metrics": snapshot.json(), "taskbar": dock.as_ref().map(|dock| dock.json()), "window": taskbar::window_diagnostics()});
    let result = std::fs::write(path, report.to_string());
    if let Some(dock) = dock { let _ = taskbar::save_preview(&snapshot, &dock, &path.with_extension("png")); }
    drop(probe);
    if com.is_ok() { unsafe { windows::Win32::System::Com::CoUninitialize(); } }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monitor_preferences_validate_and_roundtrip() {
        for interval in [1, 2, 5] {
            let options = Options { enabled: false, interval };
            assert_eq!(Options::from_json(&options.json()), options);
        }
        assert_eq!(Options::from_json(&serde_json::json!({"interval": 0})), Options::default());
        assert_eq!(Options::from_json(&serde_json::json!({"interval": 500,"enabled":false})), Options { enabled:false, interval:1 });
    }
}
