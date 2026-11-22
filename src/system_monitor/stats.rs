use super::Snapshot;
use crate::desktop::wide;
use std::collections::HashMap;
use std::time::Instant;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::NetworkManagement::IpHelper::*;
use windows_sys::Win32::System::Performance::*;
use windows_sys::Win32::System::SystemInformation::*;
use windows_sys::Win32::System::Threading::GetSystemTimes;

#[derive(Clone, Copy)]
struct CpuTimes { idle: u64, kernel: u64, user: u64 }
fn cpu_percent(previous: CpuTimes, current: CpuTimes) -> Option<f64> {
    let kernel = current.kernel.checked_sub(previous.kernel)?;
    let user = current.user.checked_sub(previous.user)?;
    let total = kernel.checked_add(user)?;
    let idle = current.idle.checked_sub(previous.idle)?;
    if total == 0 { return None; }
    Some((100.0 * total.saturating_sub(idle) as f64 / total as f64).clamp(0.0, 100.0))
}
fn cpu_times() -> Option<CpuTimes> {
    unsafe {
        let mut idle: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        if GetSystemTimes(&mut idle, &mut kernel, &mut user) == 0 { return None; }
        let ticks = |value: FILETIME| ((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64;
        Some(CpuTimes { idle: ticks(idle), kernel: ticks(kernel), user: ticks(user) })
    }
}

type Network = HashMap<u64, (u64, u64)>;
fn network_totals() -> Option<Network> {
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table) != 0 || table.is_null() { return None; }
        let mut totals = Network::new();
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
        for row in rows {
            // Count each physical uplink once, excluding loopback, tunnels,
            // virtual/filter adapters and disconnected interfaces (VPN copies).
            if row.OperStatus == 1 && row.Type != 24 && row.InterfaceAndOperStatusFlags._bitfield & 1 != 0 {
                totals.insert(row.InterfaceLuid.Value, (row.InOctets, row.OutOctets));
            }
        }
        FreeMibTable(table as _);
        Some(totals)
    }
}
fn network_rates(previous: &Network, current: &Network, seconds: f64) -> (f64, f64) {
    if !seconds.is_finite() || seconds <= 0.0 { return (0.0, 0.0); }
    let mut bytes = (0u64, 0u64);
    for (id, &(down, up)) in current {
        if let Some(&(old_down, old_up)) = previous.get(id) {
            // A reconnect/reset is a fresh baseline, never a 64-bit spike.
            bytes.0 = bytes.0.saturating_add(down.checked_sub(old_down).unwrap_or(0));
            bytes.1 = bytes.1.saturating_add(up.checked_sub(old_up).unwrap_or(0));
        }
    }
    (bytes.0 as f64 / seconds, bytes.1 as f64 / seconds)
}

struct Gpu { query: isize, counter: isize, buffer: Vec<usize> }
impl Gpu {
    fn new() -> Option<Self> {
        unsafe {
            let mut query = 0;
            if PdhOpenQueryW(std::ptr::null(), 0, &mut query) != 0 { return None; }
            let mut counter = 0;
            let path = wide(r"\GPU Engine(*)\Utilization Percentage");
            if PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) != 0 {
                PdhCloseQuery(query);
                return None;
            }
            Some(Self { query, counter, buffer: Vec::new() })
        }
    }
    fn sample(&mut self) -> Option<f64> {
        unsafe {
            if PdhCollectQueryData(self.query) != 0 { return None; }
            let (mut bytes, mut count) = (0u32, 0u32);
            PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, std::ptr::null_mut());
            if bytes == 0 || bytes > 4 * 1024 * 1024 { return None; }
            // Word-aligned storage for native pointers and doubles, reused.
            self.buffer.resize((bytes as usize + std::mem::size_of::<usize>() - 1) / std::mem::size_of::<usize>(), 0);
            let items = self.buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, items) != 0 { return None; }
            if count as usize > bytes as usize / std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>() { return None; }
            let begin = self.buffer.as_ptr() as usize;
            let end = begin + bytes as usize;
            let mut values = Vec::new();
            for item in std::slice::from_raw_parts(items, count as usize) {
                if item.FmtValue.CStatus > 1 || item.szName.is_null() { continue; }
                let address = item.szName as usize;
                if address < begin || address >= end { continue; }
                let chars = std::slice::from_raw_parts(item.szName, ((end - address) / 2).min(1024));
                let length = chars.iter().position(|c| *c == 0).unwrap_or(chars.len());
                let value = item.FmtValue.Anonymous.doubleValue;
                if value.is_finite() && value >= 0.0 { values.push((String::from_utf16_lossy(&chars[..length]), value)); }
            }
            busiest_engine(values.iter().map(|(name, value)| (name.as_str(), *value)))
        }
    }
}
impl Drop for Gpu { fn drop(&mut self) { unsafe { PdhCloseQuery(self.query); } } }

fn busiest_engine<'a>(values: impl Iterator<Item = (&'a str, f64)>) -> Option<f64> {
    let mut engines = HashMap::<&str, f64>::new();
    for (name, value) in values {
        if !value.is_finite() || value < 0.0 { continue; }
        let Some(offset) = name.find("luid_") else { continue; };
        let key = name[offset..].split("_engtype_").next().unwrap_or(&name[offset..]);
        *engines.entry(key).or_default() += value;
    }
    engines.values().copied().reduce(f64::max).map(|value| value.clamp(0.0, 100.0))
}

pub(super) struct Sampler { cpu: Option<CpuTimes>, network: Option<Network>, at: Instant, gpu: Option<Gpu>, gpu_checked: bool }
impl Sampler {
    pub fn new() -> Self { Self { cpu: None, network: None, at: Instant::now(), gpu: None, gpu_checked:false } }
    pub fn sample(&mut self) -> Snapshot {self.sample_selected(31)}
    pub fn sample_selected(&mut self, mask:u8) -> Snapshot {
        let started = Instant::now();
        let mut snapshot = Snapshot::default();
        if mask&1!=0{
            let cpu = cpu_times();
            snapshot.cpu = self.cpu.zip(cpu).and_then(|(previous, current)| cpu_percent(previous, current));
            self.cpu = cpu;
        }
        if mask&2!=0 {unsafe {
            let mut memory: MEMORYSTATUSEX = std::mem::zeroed();
            memory.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut memory) != 0 && memory.ullTotalPhys > 0 {
                snapshot.ram_total = memory.ullTotalPhys;
                snapshot.ram_used = memory.ullTotalPhys.saturating_sub(memory.ullAvailPhys);
                snapshot.ram = Some(100.0 * snapshot.ram_used as f64 / snapshot.ram_total as f64);
            }
        }}
        if mask&24!=0{
            let network = network_totals();
            if let (Some(previous), Some(current)) = (&self.network, &network) {
                let (down, up) = network_rates(previous, current, started.duration_since(self.at).as_secs_f64());
                snapshot.down = Some(down);
                snapshot.up = Some(up);
            }
            self.network = network;
            self.at = started;
        }
        if mask&4!=0 && !self.gpu_checked {self.gpu=Gpu::new();self.gpu_checked=true;}
        if mask&4!=0 {snapshot.gpu=self.gpu.as_mut().and_then(Gpu::sample);}
        snapshot.sample_ms = started.elapsed().as_secs_f64() * 1000.0;
        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_kernel_includes_idle_and_never_exceeds_total() {
        let old = CpuTimes { idle:100, kernel:200, user:100 };
        assert_eq!(cpu_percent(old, CpuTimes { idle:150,kernel:300,user:200 }), Some(75.0));
        assert_eq!(cpu_percent(old, old), None);
        assert_eq!(cpu_percent(old, CpuTimes { idle:0,kernel:0,user:0 }), None);
    }
    #[test]
    fn network_reconnect_new_adapters_and_counter_resets_do_not_spike() {
        let old = Network::from([(1,(1000,2000)),(2,(50000,60000))]);
        let current = Network::from([(1,(3000,3000)),(3,(900000000,800000000))]);
        assert_eq!(network_rates(&old,&current,2.0),(1000.0,500.0));
        assert_eq!(network_rates(&current,&old,1.0),(0.0,0.0));
        assert_eq!(network_rates(&old,&current,0.0),(0.0,0.0));
    }
    #[test]
    fn gpu_adds_processes_per_engine_then_uses_busiest_engine() {
        let values = [("pid_1_luid_abc_phys_0_eng_0_engtype_3D",40.0),("pid_2_luid_abc_phys_0_eng_0_engtype_3D",35.0),
            ("pid_1_luid_abc_phys_0_eng_1_engtype_VideoDecode",25.0),("pid_4_luid_def_phys_0_eng_0_engtype_3D",50.0)];
        assert_eq!(busiest_engine(values.into_iter()),Some(75.0));
        assert_eq!(busiest_engine(std::iter::empty()),None);
    }
    #[test]
    fn native_system_counters_return_sane_samples() {
        let mut sampler=Sampler::new();
        let _=sampler.sample();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let sample=sampler.sample();
        assert!(sample.cpu.is_some_and(|v|v.is_finite()&&(0.0..=100.0).contains(&v)));
        assert!(sample.ram_total>0&&sample.ram_used<=sample.ram_total);
        assert!(sample.ram.is_some_and(|v|(0.0..=100.0).contains(&v)));
        for rate in [sample.down,sample.up].into_iter().flatten(){assert!(rate.is_finite()&&rate>=0.0);}
        if let Some(gpu)=sample.gpu {assert!((0.0..=100.0).contains(&gpu));}
    }
}
