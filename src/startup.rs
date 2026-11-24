use std::{cell::Cell, path::Path};
use windows::{
    core::BSTR,
    Win32::{
        System::{
            Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED},
            TaskScheduler::{ITaskFolder, ITaskService, TaskScheduler, TASK_CREATE_OR_UPDATE, TASK_LOGON_INTERACTIVE_TOKEN},
            Variant::VARIANT,
        },
    },
};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS},
    System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY},
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVAL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const RUN_NAMES: [&str; 2] = ["tuanpc-desk", "DesklandRs"];

thread_local! { static ENABLED: Cell<Option<bool>> = const { Cell::new(None) }; }

fn last_error(error: &windows::core::Error) -> u32 { error.code().0 as u32 }

fn not_found(error: &windows::core::Error) -> bool {
    matches!(last_error(error), 0x80070002 | 0x80070003)
}

fn with_task<T>(f: impl FnOnce(&ITaskFolder, &BSTR, &str) -> windows::core::Result<T>) -> Result<T, String> {
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) { unsafe { CoUninitialize() }; }
    }

    let sid = unsafe { crate::desktop::current_sid() }.ok_or("Không đọc được tài khoản Windows hiện tại")?;
    let name = BSTR::from(format!("TLauncher-Logon-{sid}"));
    let result = unsafe {
        (|| {
            let status = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            // The caller may already have initialized COM as MTA.
            let _apartment = if status.is_ok() { Some(Apartment) } else if status.0 as u32 == 0x80010106 {
                None
            } else { status.ok()?; None };
            let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)?;
            let empty = VARIANT::default();
            service.Connect(&empty, &empty, &empty, &empty)?;
            let root = service.GetFolder(&BSTR::from("\\"))?;
            f(&root, &name, &sid)
        })()
    };
    result.map_err(|error: windows::core::Error| error.to_string())
}

fn task_enabled() -> Result<Option<bool>, String> {
    with_task(|root, name, _| unsafe {
        match root.GetTask(name) {
            Ok(task) => Ok(Some(task.Enabled()?.0 != 0)),
            Err(error) if not_found(&error) => Ok(None),
            Err(error) => Err(error),
        }
    })
}

fn escape_xml(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn task_xml(exe: &Path, sid: &str) -> String {
    let command = escape_xml(&exe.to_string_lossy());
    let working = escape_xml(&exe.parent().unwrap_or_else(|| Path::new(".")).to_string_lossy());
    format!(r#"<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
<RegistrationInfo><Description>Start TLauncher at sign-in for this user.</Description></RegistrationInfo>
<Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{sid}</UserId></LogonTrigger></Triggers>
<Principals><Principal id="User"><UserId>{sid}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
<Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><Priority>4</Priority></Settings>
<Actions Context="User"><Exec><Command>{command}</Command><Arguments>--autostart</Arguments><WorkingDirectory>{working}</WorkingDirectory></Exec></Actions>
</Task>"#)
}

fn register_task(exe: &Path) -> Result<(), String> {
    with_task(|root, name, sid| unsafe {
        let empty = VARIANT::default();
        let user = VARIANT::from(BSTR::from(sid));
        root.RegisterTask(name, &BSTR::from(task_xml(exe, sid)), TASK_CREATE_OR_UPDATE.0,
            &user, &empty, TASK_LOGON_INTERACTIVE_TOKEN, &empty)?;
        Ok(())
    })
}

fn remove_task() -> Result<(), String> {
    with_task(|root, name, _| unsafe {
        match root.DeleteTask(name, 0) {
            Ok(()) => Ok(()),
            Err(error) if not_found(&error) => Ok(()),
            Err(error) => Err(error),
        }
    })
}

fn run_value(name: &str) -> Option<String> {
    crate::lock::reg_get_string(HKEY_CURRENT_USER, RUN_KEY, name)
}

// Honor Windows' disabled Startup Apps entries; only a deliberate toggle in
// TLauncher may opt in again. Missing approval is the legacy enabled default.
fn approved(name: &str) -> bool {
    let key = crate::desktop::wide(APPROVAL_KEY);
    let name = crate::desktop::wide(name);
    let mut buffer = [0u8; 32];
    let mut len = buffer.len() as u32;
    let result = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_BINARY,
        std::ptr::null_mut(), buffer.as_mut_ptr().cast(), &mut len) };
    match result {
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => true,
        ERROR_SUCCESS => len >= 4 && matches!(buffer[0], 2 | 6),
        _ => false,
    }
}

fn legacy_enabled() -> bool {
    RUN_NAMES.iter().any(|name| run_value(name).is_some() && approved(name))
}

fn clear_legacy() {
    for name in RUN_NAMES {
        if !crate::lock::reg_delete_value(HKEY_CURRENT_USER, RUN_KEY, name) {
            crate::log!("Không xóa được mục khởi động cũ: {name}");
        }
    }
}

pub(crate) fn enabled() -> bool {
    ENABLED.with(|cached| cached.get()).unwrap_or_else(legacy_enabled)
}

pub(crate) fn set(on: bool) {
    if let Err(error) = crate::trial::protect_windows() { crate::log!("{error}"); return; }
    let result = if on {
        std::env::current_exe().map_err(|e| e.to_string()).and_then(|exe| register_task(&exe))
    } else {
        // Remove the Run fallback even when the scheduler is unavailable.
        clear_legacy();
        remove_task()
    };
    match result {
        Ok(()) => {
            if on { clear_legacy(); }
            ENABLED.with(|cached| cached.set(Some(on || legacy_enabled())));
            crate::log!("Khởi động cùng Windows (Task Scheduler): {}", if on { "bật" } else { "tắt" });
        }
        Err(error) => {
            crate::log!("Không đổi được khởi động cùng Windows: {error}");
            let actual = task_enabled().ok().flatten().unwrap_or_else(|| on && enabled() || legacy_enabled());
            ENABLED.with(|cached| cached.set(Some(actual)));
        }
    }
}

pub(crate) fn migrate() {
    if crate::trial::protect_windows().is_err() { return; }
    match task_enabled() {
        Ok(Some(on)) => {
            ENABLED.with(|cached| cached.set(Some(on || legacy_enabled())));
            // A disabled task might be user policy; do not remove the fallback
            // or re-enable the task on the user's behalf.
            if on { clear_legacy(); }
        }
        Ok(None) if legacy_enabled() => set(true),
        Ok(None) => ENABLED.with(|cached| cached.set(Some(false))),
        Err(error) => crate::log!("Chưa đọc được tác vụ khởi động: {error}; giữ cấu hình cũ"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logon_task_uses_current_user_without_delay_or_admin_privileges() {
        let xml = task_xml(Path::new(r"C:\Tools & Utilities\TLauncher.exe"), "S-1-5-21-123");
        assert_eq!(xml.matches("<UserId>S-1-5-21-123</UserId>").count(), 2);
        assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(!xml.contains("<Delay>"));
        assert!(xml.contains("<Priority>4</Priority>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
        assert!(xml.contains(r"<Command>C:\Tools &amp; Utilities\TLauncher.exe</Command>"));
        assert!(xml.contains(r"<WorkingDirectory>C:\Tools &amp; Utilities</WorkingDirectory>"));
    }
}
