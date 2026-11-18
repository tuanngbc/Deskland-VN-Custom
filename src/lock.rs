//! Màn hình khóa - CHỈ dùng cơ chế khóa thật của Windows.
//!
//! Nguyên tắc (khác hẳn bản C#):
//! - KHÔNG hook bàn phím, KHÔNG chặn Win+L, KHÔNG ghi `DisableLockWorkstation`.
//! - KHÔNG tự kiểm mật khẩu / PIN, KHÔNG lưu bất kỳ mã nào.
//! - Khóa = `LockWorkStation()`: Windows chuyển sang desktop an toàn (Winlogon), mở khóa bằng
//!   mật khẩu / PIN / Windows Hello của chính Windows.
//! - Cảnh động khi máy rảnh = trình bảo vệ màn hình Windows (`.scr`) với `ScreenSaverIsSecure=1`:
//!   Windows tự khóa phiên khi người dùng quay lại; Deskland chỉ vẽ, không quyết định khóa / mở.
//! - Ảnh màn khóa: chụp cảnh hiện tại, đặt qua API WinRT `LockScreen.SetImageFileAsync`.

use crate::desktop::wide;
use std::os::windows::process::CommandExt;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Shutdown::LockWorkStation;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const POLICY_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Policies\System";
const DESKTOP_KEY: &str = r"Control Panel\Desktop";
const SAVER_POLICY_KEY: &str = r"Software\Policies\Microsoft\Windows\Control Panel\Desktop";
const SAVER_MARKER: &str = "DesklandRsSaverInstalled";

/// Thư mục dữ liệu của bản Rust (ngoài AppData để tránh ảo hoá MSIX khi chạy từ ứng dụng đóng gói).
pub fn data_dir() -> std::path::PathBuf {
    let base = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
    let d = std::path::Path::new(&base).join(".deskland-rs");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn lock_now() {
    crate::log!("Khóa máy bằng LockWorkStation() (khóa thật của Windows)");
    if unsafe { LockWorkStation() } == 0 {
        crate::log!("LockWorkStation thất bại: lỗi {}", unsafe { GetLastError() });
    }
}

fn reg_get_dword(root: HKEY, key: &str, name: &str) -> Option<u32> {
    let (k, n) = (wide(key), wide(name));
    let mut v = 0u32;
    let mut sz = 4u32;
    let r = unsafe { RegGetValueW(root, k.as_ptr(), n.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), &mut v as *mut u32 as *mut _, &mut sz) };
    (r == ERROR_SUCCESS).then_some(v)
}

pub(crate) fn reg_get_string(root: HKEY, key: &str, name: &str) -> Option<String> {
    let (k, n) = (wide(key), wide(name));
    let mut buf = vec![0u16; 1024];
    let mut sz = (buf.len() * 2) as u32;
    let r = unsafe { RegGetValueW(root, k.as_ptr(), n.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr() as *mut _, &mut sz) };
    if r != ERROR_SUCCESS {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&buf[..len]))
}

pub(crate) fn reg_set_string(root: HKEY, key: &str, name: &str, val: &str) -> bool {
    let (k, n, v) = (wide(key), wide(name), wide(val));
    let r = unsafe { RegSetKeyValueW(root, k.as_ptr(), n.as_ptr(), REG_SZ, v.as_ptr() as *const _, (v.len() * 2) as u32) };
    r == ERROR_SUCCESS
}

/// Bản C# cũ ghi `DisableLockWorkstation=1` để tự xử lý Win+L - việc này TẮT khóa thật của Windows.
pub fn winl_disabled() -> bool {
    reg_get_dword(HKEY_CURRENT_USER, POLICY_KEY, "DisableLockWorkstation").is_some_and(|v| v != 0)
}

/// Xoá giá trị `DisableLockWorkstation` (chỉ khi người dùng bấm trong menu khay).
pub fn restore_winl() -> bool {
    let (k, n) = (wide(POLICY_KEY), wide("DisableLockWorkstation"));
    let r = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr()) };
    let ok = r == ERROR_SUCCESS || r == ERROR_FILE_NOT_FOUND;
    crate::log!("Khôi phục Win+L của Windows: {}", if ok { "xong" } else { "thất bại" });
    ok
}

/// Tên tệp hiện trong danh sách trình bảo vệ màn hình của Windows
fn saver_path() -> std::path::PathBuf {
    data_dir().join("Deskland-VN.scr")
}

/// Tên cũ (trước bản 1.0.0) - vẫn nhận là của mình để bật / gỡ đúng
fn saver_path_old() -> std::path::PathBuf {
    data_dir().join("Deskland.scr")
}

fn saver_backup_path() -> std::path::PathBuf {
    data_dir().join("saver-registry-backup.json")
}

fn save_saver_backup() -> Result<(), String> {
    if saver_backup_path().is_file() { return Ok(()); }
    let value = serde_json::json!({
        "scrnsave": reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "SCRNSAVE.EXE"),
        "active": reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveActive"),
        "timeout": reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveTimeOut"),
        "secure": reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaverIsSecure"),
        "marker": reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, SAVER_MARKER),
    });
    let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    std::fs::write(saver_backup_path(), bytes).map_err(|e| format!("Không lưu được cài đặt saver cũ: {e}"))
}

fn restore_saver_backup() -> bool {
    let path = saver_backup_path();
    let Ok(bytes) = std::fs::read(&path) else { return false; };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return false; };
    if !["scrnsave", "active", "timeout", "secure", "marker"].iter()
        .all(|key| value.get(*key).is_some_and(|v| v.is_null() || v.is_string())) { return false; }
    let mut ok = true;
    for (key, name) in [("scrnsave", "SCRNSAVE.EXE"), ("active", "ScreenSaveActive"),
        ("timeout", "ScreenSaveTimeOut"), ("secure", "ScreenSaverIsSecure"), ("marker", SAVER_MARKER)] {
        if let Some(v) = value.get(key).and_then(|v| v.as_str()) {
            ok &= reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, name, v);
        } else {
            ok &= reg_delete_value(HKEY_CURRENT_USER, DESKTOP_KEY, name);
        }
    }
    ok
}

fn runtime_value(action: u32) -> Result<u32, String> {
    let mut value = 0u32;
    if unsafe { SystemParametersInfoW(action, 0, (&mut value as *mut u32).cast(), 0) } == 0 {
        return Err(format!("Không đọc được trạng thái Windows (SPI=0x{action:X}, lỗi {})", unsafe { GetLastError() }));
    }
    Ok(value)
}

// Registry và trạng thái trong phiên Windows là hai dữ liệu khác nhau. Ghi
// Registry hoặc broadcast WM_SETTINGCHANGE đơn thuần không tắt timer Winlogon.
fn set_saver_parameter(set: u32, get: u32, value: u32) -> Result<(), String> {
    if unsafe { SystemParametersInfoW(set, value, std::ptr::null_mut(), SPIF_UPDATEINIFILE | SPIF_SENDCHANGE) } == 0 {
        return Err(format!("Windows chưa áp dụng cài đặt màn hình chờ (SPI=0x{set:X}, lỗi {})", unsafe { GetLastError() }));
    }
    let actual = runtime_value(get)?;
    if actual != value {
        return Err(format!("Windows chưa xác nhận cài đặt màn hình chờ: yêu cầu {value}, thực tế {actual} (SPI=0x{get:X})"));
    }
    Ok(())
}

fn set_saver_active(active: bool) -> Result<(), String> {
    set_saver_parameter(SPI_SETSCREENSAVEACTIVE, SPI_GETSCREENSAVEACTIVE, active as u32)
}

fn is_our_saver(path: &str) -> bool {
    let path = path.trim().trim_matches('"');
    path.eq_ignore_ascii_case(&saver_path().to_string_lossy())
        || path.eq_ignore_ascii_case(&saver_path_old().to_string_lossy())
}

fn needs_disabled_repair(path: &str, known_deskland: bool, stored_active: bool, runtime_active: bool) -> bool {
    known_deskland && !stored_active && runtime_active && (path.trim().is_empty() || is_our_saver(path))
}

/// Sửa bản cũ đã ghi "tắt" trong Registry nhưng vẫn để timer Winlogon bật.
/// Không bật saver khi khởi động, không đổi saver của phần mềm khác.
pub fn reconcile_disabled_saver() -> Result<(), String> {
    if reg_get_string(HKEY_CURRENT_USER, SAVER_POLICY_KEY, "ScreenSaveActive").is_some() { return Ok(()); }
    let path = reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "SCRNSAVE.EXE").unwrap_or_default();
    let marker = reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, SAVER_MARKER);
    let stored_off = reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveActive").is_some_and(|v| v.trim() == "0");
    let known_deskland = marker.is_some_and(|v| v == "0" || v == "1") || is_our_saver(&path);
    if needs_disabled_repair(&path, known_deskland, !stored_off, runtime_value(SPI_GETSCREENSAVEACTIVE)? != 0) {
        crate::log!("Sửa lệch trạng thái saver: Registry=tắt nhưng Winlogon=bật");
        set_saver_active(false)?;
        crate::log!("Đã xác nhận timer màn hình chờ Windows tắt (SPI_GETSCREENSAVEACTIVE=0)");
    }
    Ok(())
}

pub fn log_saver_state(reason: &str) {
    crate::log!("Saver [{reason}]: runtime active={:?}, timeout={:?}, secure={:?}, running={:?}; Registry active={:?}, path={:?}, marker={:?}",
        runtime_value(SPI_GETSCREENSAVEACTIVE), runtime_value(SPI_GETSCREENSAVETIMEOUT),
        runtime_value(SPI_GETSCREENSAVESECURE), runtime_value(SPI_GETSCREENSAVERRUNNING),
        reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveActive"),
        reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "SCRNSAVE.EXE"),
        reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, SAVER_MARKER));
}

/// Trình bảo vệ màn hình hiện tại có phải của Deskland Rust không.
pub fn saver_installed() -> bool {
    let cur = reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "SCRNSAVE.EXE").unwrap_or_default();
    is_our_saver(&cur) || (cur.trim().is_empty()
        && reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, SAVER_MARKER).is_some_and(|v| v == "1"))
}

/// Đặt Deskland làm trình bảo vệ màn hình, BẮT BUỘC kèm "khi tiếp tục, hiện màn hình đăng nhập".
pub fn install_saver(idle_mins: u32) -> Result<(), String> {
    if let Some(p) = reg_get_string(HKEY_CURRENT_USER, SAVER_POLICY_KEY, "SCRNSAVE.EXE") {
        return Err(format!("Chính sách nhóm đang ép trình bảo vệ màn hình '{p}', không đổi được"));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dst = saver_path();
    // Chỉ lưu cấu hình bên ngoài Deskland; nếu đang cập nhật saver của chính
    // mình thì không được ghi đè bản sao gốc bằng đường dẫn Deskland.
    if !saver_installed() { save_saver_backup()?; }
    std::fs::copy(&exe, &dst).map_err(|e| format!("Chép {} lỗi: {e}", dst.display()))?;
    let secs = (idle_mins.max(1) * 60).to_string();
    let ok = reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "SCRNSAVE.EXE", &dst.to_string_lossy())
        && reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveActive", "1")
        && reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveTimeOut", &secs)
        && reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaverIsSecure", "1")
        && reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, SAVER_MARKER, "1");
    if !ok {
        return Err("Ghi cài đặt trình bảo vệ màn hình thất bại".into());
    }
    // Đặt thời gian và bảo mật trước, chỉ bật timer sau khi cả hai thành công.
    set_saver_parameter(SPI_SETSCREENSAVETIMEOUT, SPI_GETSCREENSAVETIMEOUT, idle_mins.max(1) * 60)?;
    set_saver_parameter(SPI_SETSCREENSAVESECURE, SPI_GETSCREENSAVESECURE, 1)?;
    set_saver_active(true)?;
    log_saver_state("bật");
    crate::log!("Đã đặt trình bảo vệ màn hình {} sau {idle_mins} phút, có khóa khi quay lại", dst.display());
    Ok(())
}

/// Trình bảo vệ màn hình hiện tại có bật "khi tiếp tục, hiện màn hình đăng nhập" không.
pub fn saver_secure() -> bool {
    runtime_value(SPI_GETSCREENSAVESECURE).is_ok_and(|v| v != 0)
}

/// Trình bảo vệ màn hình Deskland hiện vừa được cài vừa đang được Windows bật.
/// `saver_installed()` chỉ nói về chủ sở hữu cấu hình; Windows có thể đang để
/// `ScreenSaveActive=0` (ví dụ người dùng chọn Never). Không được coi trường
/// hợp đó là tín hiệu để tự khóa phiên.
pub fn saver_active() -> bool {
    saver_installed()
        && reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveActive")
            .is_some_and(|v| v.trim() == "1")
        && runtime_value(SPI_GETSCREENSAVEACTIVE).is_ok_and(|v| v != 0)
}

/// Windows có đang chạy trình bảo vệ màn hình (bất kỳ) không.
pub fn saver_running() -> bool {
    let mut on: BOOL = 0;
    unsafe { SystemParametersInfoW(SPI_GETSCREENSAVERRUNNING, 0, &mut on as *mut BOOL as *mut _, 0) };
    on != 0
}

/// Desktop nhận bàn phím có phải desktop thường ("Default") không. Khi khóa máy, UAC hay
/// Ctrl+Alt+Del, input desktop là "Winlogon" (mở không được hoặc tên khác) -> desktop không ai thấy.
pub fn input_desktop_is_default() -> bool {
    use windows_sys::Win32::System::StationsAndDesktops::*;
    unsafe {
        let d = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS);
        if d.is_null() {
            return false;
        }
        let mut buf = [0u16; 64];
        let mut need = 0u32;
        let ok = GetUserObjectInformationW(d as HANDLE, UOI_NAME, buf.as_mut_ptr() as *mut _, (buf.len() * 2) as u32, &mut need) != 0;
        CloseDesktop(d);
        let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
        !ok || String::from_utf16_lossy(&buf[..len]).eq_ignore_ascii_case("Default")
    }
}

/// "Khóa với cảnh động": nhờ CHÍNH Windows bật trình bảo vệ màn hình đã cài (SC_SCREENSAVE).
/// Vì `ScreenSaverIsSecure=1`, Windows coi đây là trạng thái an toàn và tự hiện màn đăng nhập khi
/// có thao tác. Deskland không tự chạy file .scr (như vậy Windows không biết để khóa).
pub fn start_secure_saver(hwnd: HWND) {
    crate::log!("Nhờ Windows bật trình bảo vệ màn hình (SC_SCREENSAVE)");
    unsafe {
        SetForegroundWindow(hwnd);
        SendMessageW(hwnd, WM_SYSCOMMAND, 0xF140usize /* SC_SCREENSAVE */, 0);
    }
}

pub fn preview_saver() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe).args(["--saver", "--fps", "6"])
        .creation_flags(0x0800_0000).spawn().map_err(|e| e.to_string())?;
    crate::log!("Xem thử màn hình chờ không khóa, không thay đổi timer Windows");
    Ok(())
}

/// Gỡ saver của mình; thao tác lặp lại vẫn sửa được timer cũ còn bật trong RAM.
pub fn uninstall_saver() -> Result<(), String> {
    log_saver_state("trước tắt");
    if saver_installed() {
        // Tắt Winlogon trước khi xóa marker. Nếu Windows từ chối (đang khóa),
        // báo lỗi để giao diện không thông báo thành công giả.
        set_saver_active(false)?;
        let had_backup = saver_backup_path().is_file();
        let restored = restore_saver_backup();
        if had_backup && !restored {
            return Err("Timer đã tắt nhưng chưa khôi phục được bản sao cài đặt cũ; giữ bản sao để thử lại".into());
        }
        if !restored {
            let ok = reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "SCRNSAVE.EXE", "")
            // SPI_SETSCREENSAVEACTIVE không phải lúc nào cũng ghi ngược giá trị
            // registry ngay (đặc biệt khi tắt từ WebView2), nên ghi rõ trạng thái 0.
                && reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveActive", "0")
                && reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, SAVER_MARKER, "0");
            if !ok { return Err("Timer đã tắt nhưng chưa lưu được trạng thái tắt vào cấu hình Windows".into()); }
        }
        if restored {
            if let Some(timeout) = reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveTimeOut").and_then(|v| v.parse::<u32>().ok()) {
                set_saver_parameter(SPI_SETSCREENSAVETIMEOUT, SPI_GETSCREENSAVETIMEOUT, timeout)?;
            }
            if let Some(secure) = reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaverIsSecure") {
                set_saver_parameter(SPI_SETSCREENSAVESECURE, SPI_GETSCREENSAVESECURE, (secure.trim() == "1") as u32)?;
            }
            let path = reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "SCRNSAVE.EXE").unwrap_or_default();
            let active = !path.trim().is_empty() && !is_our_saver(&path)
                && reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveActive").is_some_and(|v| v.trim() == "1");
            set_saver_active(active)?;
            if is_our_saver(&path) {
                // Bản sao cũ không được cài lại Deskland vừa yêu cầu gỡ.
                if !reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "SCRNSAVE.EXE", "")
                    || !reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, SAVER_MARKER, "0") {
                    return Err("Chưa bỏ được đường dẫn saver Deskland trong cài đặt cũ".into());
                }
            }
            let _ = std::fs::remove_file(saver_backup_path());
        }
        crate::log!("Đã gỡ trình bảo vệ màn hình Deskland-VN");
    } else {
        reconcile_disabled_saver()?;
    }
    log_saver_state("sau tắt");
    Ok(())
}

/// Đặt ảnh màn khóa Windows từ ảnh BMP (chạy nền, không chặn vòng vẽ).
/// Chuyển BMP -> JPG rồi gọi WinRT `Windows.System.UserProfile.LockScreen.SetImageFileAsync`.
pub fn sync_lock_image(bmp: &std::path::Path) {
    let jpg = bmp.with_extension("jpg");
    let esc = |p: &std::path::Path| p.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; Add-Type -AssemblyName System.Drawing; \
         $b=[System.Drawing.Bitmap]::FromFile('{bmp}'); $b.Save('{jpg}',[System.Drawing.Imaging.ImageFormat]::Jpeg); $b.Dispose(); Remove-Item -LiteralPath '{bmp}'; \
         Add-Type -AssemblyName System.Runtime.WindowsRuntime; \
         $g=([System.WindowsRuntimeSystemExtensions].GetMethods()|?{{$_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'}})[0]; \
         $a=([System.WindowsRuntimeSystemExtensions].GetMethods()|?{{$_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncAction'}})[0]; \
         [Windows.Storage.StorageFile,Windows.Storage,ContentType=WindowsRuntime]|Out-Null; \
         [Windows.System.UserProfile.LockScreen,Windows.System.UserProfile,ContentType=WindowsRuntime]|Out-Null; \
         $f=$g.MakeGenericMethod([Windows.Storage.StorageFile]).Invoke($null,@([Windows.Storage.StorageFile]::GetFileFromPathAsync('{jpg}'))).GetAwaiter().GetResult(); \
         $a.Invoke($null,@([Windows.System.UserProfile.LockScreen]::SetImageFileAsync($f))).GetAwaiter().GetResult()",
        bmp = esc(bmp),
        jpg = esc(&jpg)
    );
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    match std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => crate::log!("Đang đặt ảnh màn khóa Windows: {}", jpg.display()),
        Err(e) => crate::log!("Không chạy được PowerShell để đặt ảnh màn khóa: {e}"),
    }
}

/// Đặt ảnh BMP làm hình nền tĩnh của Windows (kiểu Fill, chạy ở luồng riêng vì SPIF_SENDCHANGE phát
/// thông điệp đồng bộ). Lúc mở máy, trước khi app kịp chạy, desktop hiện đúng cảnh này thay vì màn đen.
pub fn set_static_wallpaper(bmp: std::path::PathBuf) {
    std::thread::spawn(move || {
        reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "WallpaperStyle", "10");
        reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "TileWallpaper", "0");
        let mut w = wide(&bmp.to_string_lossy());
        let ok = unsafe {
            SystemParametersInfoW(SPI_SETDESKWALLPAPER, 0, w.as_mut_ptr().cast(), SPIF_UPDATEINIFILE | SPIF_SENDCHANGE)
        };
        if ok != 0 {
            crate::log!("Đã đặt hình nền tĩnh Windows từ cảnh hiện tại: {}", bmp.display());
        } else {
            crate::log!("Đặt hình nền tĩnh Windows thất bại");
        }
    });
}

pub(crate) fn reg_delete_value(root: HKEY, key: &str, name: &str) -> bool {
    let (k, n) = (wide(key), wide(name));
    let r = unsafe { RegDeleteKeyValueW(root, k.as_ptr(), n.as_ptr()) };
    r == ERROR_SUCCESS || r == ERROR_FILE_NOT_FOUND
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_repair_only_applies_to_owned_disabled_savers() {
        let ours = saver_path().to_string_lossy().into_owned();
        assert!(needs_disabled_repair("", true, false, true));
        assert!(needs_disabled_repair(&ours, true, false, true));
        assert!(!needs_disabled_repair("", false, false, true));
        assert!(!needs_disabled_repair("", true, true, true));
        assert!(!needs_disabled_repair("", true, false, false));
        assert!(!needs_disabled_repair(r"C:\Windows\System32\Mystify.scr", true, false, true));
    }

    // Chỉ chạy trong Windows runner dùng một lần. Không tự đổi thiết lập máy
    // cá nhân khi chạy cargo test thông thường.
    #[test]
    #[ignore = "requires an isolated Windows CI session"]
    fn saver_disable_repairs_stale_winlogon_state() {
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        struct Restore {
            active: u32,
            values: Vec<(&'static str, Option<String>)>,
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = set_saver_active(self.active != 0);
                for (name, value) in &self.values {
                    if let Some(value) = value { reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, name, value); }
                    else { reg_delete_value(HKEY_CURRENT_USER, DESKTOP_KEY, name); }
                }
            }
        }
        let _restore = Restore {
            active: runtime_value(SPI_GETSCREENSAVEACTIVE).unwrap(),
            values: ["SCRNSAVE.EXE", "ScreenSaveActive", SAVER_MARKER].into_iter()
                .map(|name| (name, reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, name))).collect(),
        };

        // Tái tạo dữ liệu thực tế: timer còn bật trong RAM, đường dẫn đã xóa,
        // marker=0 và ScreenSaveActive=0 khiến uninstall cũ bỏ qua hoàn toàn.
        set_saver_active(true).unwrap();
        assert!(reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "SCRNSAVE.EXE", ""));
        assert!(reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, SAVER_MARKER, "0"));
        assert!(reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveActive", "0"));
        assert_eq!(runtime_value(SPI_GETSCREENSAVEACTIVE).unwrap(), 1);
        assert!(!saver_installed());
        reconcile_disabled_saver().unwrap();
        assert_eq!(runtime_value(SPI_GETSCREENSAVEACTIVE).unwrap(), 0);
        assert_eq!(reg_get_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveActive").as_deref(), Some("0"));

        set_saver_active(true).unwrap();
        assert!(reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveActive", "0"));
        uninstall_saver().unwrap();
        uninstall_saver().unwrap();
        assert_eq!(runtime_value(SPI_GETSCREENSAVEACTIVE).unwrap(), 0);

        // Marker Deskland cũ không cho phép chạm saver mới của ứng dụng khác.
        set_saver_active(true).unwrap();
        assert!(reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "ScreenSaveActive", "0"));
        assert!(reg_set_string(HKEY_CURRENT_USER, DESKTOP_KEY, "SCRNSAVE.EXE", r"C:\Windows\System32\Mystify.scr"));
        reconcile_disabled_saver().unwrap();
        assert!(!saver_installed());
        assert_eq!(runtime_value(SPI_GETSCREENSAVEACTIVE).unwrap(), 1);
    }
}
