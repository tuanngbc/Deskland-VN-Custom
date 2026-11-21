//! Atomic, preserving writes for the existing settings owner. No profile globals.
use serde_json::Value;
use std::{io::{self, Write}, path::Path, sync::atomic::{AtomicU64, Ordering}};
static SERIAL: AtomicU64 = AtomicU64::new(0);

fn merge(current: &mut Value, patch: Value) {
    if let (Some(a),Some(b))=(current.as_object_mut(),patch.as_object()) {
        for (key,value) in b {merge(a.entry(key).or_insert(Value::Null),value.clone());}
    } else {*current=patch;}
}

pub fn save(path: &Path, patch: Value) -> io::Result<()> {
    let mut current = match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}'))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData,error))?,
        Err(error) if error.kind()==io::ErrorKind::NotFound => serde_json::json!({}),
        Err(error) => return Err(error),
    };
    if !current.is_object() || !patch.is_object() {return Err(io::Error::new(io::ErrorKind::InvalidData,"settings root must be an object"));}
    merge(&mut current,patch);
    let bytes=serde_json::to_vec_pretty(&current)?;
    let parent=path.parent().ok_or_else(||io::Error::new(io::ErrorKind::InvalidInput,"settings path has no directory"))?;
    std::fs::create_dir_all(parent)?;
    let name=path.file_name().ok_or_else(||io::Error::new(io::ErrorKind::InvalidInput,"settings path has no filename"))?.to_string_lossy();
    let temporary=parent.join(format!(".{name}.{}-{}.tmp",std::process::id(),SERIAL.fetch_add(1,Ordering::Relaxed)));
    let result=(|| {
        let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        file.write_all(&bytes)?;file.sync_all()?;drop(file);
        let from=crate::desktop::wide(&temporary.to_string_lossy());let to=crate::desktop::wide(&path.to_string_lossy());
        unsafe {
            use windows_sys::Win32::Storage::FileSystem::{MoveFileExW,MOVEFILE_REPLACE_EXISTING,MOVEFILE_WRITE_THROUGH};
            if MoveFileExW(from.as_ptr(),to.as_ptr(),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH)==0 {return Err(io::Error::last_os_error());}
        }
        Ok(())
    })();
    if result.is_err() {let _=std::fs::remove_file(&temporary);}
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_writes_preserve_existing_modules_and_unknown_fields() {
        let dir=std::env::temp_dir().join(format!("tuanpc-config-test-{}-{}",std::process::id(),SERIAL.fetch_add(1,Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();let path=dir.join("settings.json");
        let original=serde_json::json!({"theme":"beach","params":{"beach":{"seed":123}},"widget":{"x":-800,"future":true},"launcherHotkey":{"vk":32},"futureModule":{"keep":true}});
        std::fs::write(&path,original.to_string()).unwrap();
        save(&path,serde_json::json!({"dock":{"enabled":true},"widget":{"x":-700}})).unwrap();
        let actual:Value=serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(actual["theme"],original["theme"]);assert_eq!(actual["params"],original["params"]);
        assert_eq!(actual["launcherHotkey"],original["launcherHotkey"]);assert_eq!(actual["futureModule"],original["futureModule"]);
        assert_eq!(actual["widget"]["future"],true);assert_eq!(actual["widget"]["x"],-700);assert_eq!(actual["dock"]["enabled"],true);
        // Invalid existing data must remain byte-for-byte recoverable.
        std::fs::write(&path,"{broken").unwrap();assert!(save(&path,serde_json::json!({"dock":{}})).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(),"{broken");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(),1);
        std::fs::remove_file(path).unwrap();std::fs::remove_dir(dir).unwrap();
    }
}
