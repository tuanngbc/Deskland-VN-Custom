//! Shared shortcut validation, display and reversible Windows registration.
use windows_sys::Win32::Foundation::{GetLastError, HWND};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub fn normalize(mods: u32, vk: u32) -> Option<u32> {
    let allowed = MOD_ALT | MOD_CONTROL | MOD_SHIFT;
    let valid_key = [8, 9, 13, 32, 33, 34, 35, 36, 37, 38, 39, 40, 45, 46].contains(&vk)
        || (0x30..=0x39).contains(&vk) || (0x41..=0x5a).contains(&vk)
        || (0x60..=0x69).contains(&vk) || (0x70..=0x87).contains(&vk);
    (mods & allowed != 0 && mods & !(allowed | MOD_NOREPEAT) == 0
        && valid_key && vk != VK_F12 as u32).then_some((mods & allowed) | MOD_NOREPEAT)
}

pub fn label(mods: u32, vk: u32) -> String {
    let mut parts = Vec::new();
    if mods & MOD_CONTROL != 0 { parts.push("Ctrl".into()); }
    if mods & MOD_ALT != 0 { parts.push("Alt".into()); }
    if mods & MOD_SHIFT != 0 { parts.push("Shift".into()); }
    parts.push(match vk {
        8 => "Backspace".into(), 9 => "Tab".into(), 13 => "Enter".into(), 32 => "Space".into(),
        33 => "PageUp".into(), 34 => "PageDown".into(), 35 => "End".into(), 36 => "Home".into(),
        37 => "Left".into(), 38 => "Up".into(), 39 => "Right".into(), 40 => "Down".into(),
        45 => "Insert".into(), 46 => "Delete".into(),
        0x30..=0x39 | 0x41..=0x5a => char::from_u32(vk).unwrap().to_string(),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6f),
        _ => format!("VK 0x{vk:X}"),
    });
    parts.join("+")
}

/// A failed replacement must leave the old binding registered.
pub fn replace(hwnd: HWND, id: i32, old: (u32, u32), new: (u32, u32)) -> Result<(), String> {
    let Some(mods) = normalize(new.0, new.1) else { return Err("Tổ hợp không hợp lệ. Dùng Ctrl, Alt hoặc Shift cùng một phím; F12 dành riêng cho Windows.".into()); };
    unsafe {
        UnregisterHotKey(hwnd, id);
        if RegisterHotKey(hwnd, id, mods, new.1) != 0 { return Ok(()); }
        let error = GetLastError();
        let restored = RegisterHotKey(hwnd, id, old.0, old.1) != 0;
        crate::log!("Không đổi được phím tắt {}: lỗi {error}; khôi phục phím cũ={restored}", label(mods, new.1));
        Err(if restored {
            format!("{} đang được Windows hoặc ứng dụng khác sử dụng (lỗi {error}). Đã giữ phím cũ.", label(mods, new.1))
        } else {
            format!("Windows từ chối phím mới và không khôi phục được phím cũ (lỗi {error}). Hãy chọn tổ hợp khác.")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_shortcuts_cover_single_modifiers_and_named_keys() {
        for mods in 1..=7 { for key in [8,9,13,32,37,46,48,65,90,96,112,122,124,135] {
            assert_eq!(normalize(mods,key),Some(mods|MOD_NOREPEAT));
            assert!(!label(mods,key).contains("VK"));
        }}
        for (mods,key) in [(0,65),(8,65),(9,65),(2,123),(2,0),(2,16)] { assert_eq!(normalize(mods,key),None); }
        assert_eq!(label(MOD_ALT|MOD_NOREPEAT,32),"Alt+Space");
    }
    #[test]
    #[ignore = "requires an isolated Windows CI session"]
    fn windows_conflict_restores_previous_registration() {
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        let hwnd = std::ptr::null_mut();
        let mods = MOD_CONTROL|MOD_ALT|MOD_SHIFT|MOD_NOREPEAT;
        struct Cleanup;
        impl Drop for Cleanup { fn drop(&mut self) { unsafe { for id in [201,202,203] { UnregisterHotKey(std::ptr::null_mut(),id); } } } }
        let _cleanup = Cleanup;
        let (ready_tx,ready_rx)=std::sync::mpsc::channel();
        let (stop_tx,stop_rx)=std::sync::mpsc::channel();
        let thread=std::thread::spawn(move || unsafe {
            let ok=RegisterHotKey(std::ptr::null_mut(),202,mods,VK_F24 as u32)!=0;
            ready_tx.send(ok).unwrap();
            if ok { let _=stop_rx.recv();UnregisterHotKey(std::ptr::null_mut(),202); }
        });
        struct Blocker { stop: Option<std::sync::mpsc::Sender<()>>, thread: Option<std::thread::JoinHandle<()>> }
        impl Drop for Blocker {fn drop(&mut self){if let Some(stop)=self.stop.take(){let _=stop.send(());}if let Some(thread)=self.thread.take(){let _=thread.join();}}}
        let blocker=Blocker{stop:Some(stop_tx),thread:Some(thread)};
        assert!(ready_rx.recv().unwrap(),"runner must allow a conflicting registration");
        let available=move |vk| std::thread::spawn(move || unsafe {
            let ok=RegisterHotKey(std::ptr::null_mut(),203,mods,vk)!=0;
            if ok {UnregisterHotKey(std::ptr::null_mut(),203);}
            ok
        }).join().unwrap();
        unsafe {
            assert_ne!(RegisterHotKey(hwnd,201,mods,VK_F23 as u32),0);
            assert!(replace(hwnd,201,(mods,VK_F23 as u32),(mods,VK_F24 as u32)).is_err());
            assert!(!available(VK_F23 as u32),"old shortcut must still be owned");
            drop(blocker);
            replace(hwnd,201,(mods,VK_F23 as u32),(mods,VK_F24 as u32)).unwrap();
            assert!(available(VK_F23 as u32),"old shortcut must now be free");
            assert!(!available(VK_F24 as u32),"new shortcut must now be owned");
        }
    }
}
