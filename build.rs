// Nhúng app.manifest + tài nguyên app.rc (app.ico & VERSIONINFO) vào exe (trình liên kết MSVC).
// Không có manifest thì cửa sổ con phân lớp của chế độ 24H2+ không hiện lên màn hình (desktop đen).
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_rc_exe() -> Option<PathBuf> {
    if Command::new("rc.exe").arg("/?").output().is_ok() {
        return Some(PathBuf::from("rc.exe"));
    }
    let pf86 = std::env::var_os("ProgramFiles(x86)").unwrap_or_else(|| r"C:\Program Files (x86)".into());
    let kits_bin = Path::new(&pf86).join("Windows Kits").join("10").join("bin");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&kits_bin)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    // Ưu tiên phiên bản Windows SDK mới nhất
    versions.sort_by(|a, b| b.cmp(a));
    for ver in versions {
        for arch in ["x64", "arm64", "x86"] {
            let cand = ver.join(arch).join("rc.exe");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/app.ico");

    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows") && target.contains("msvc") {
        println!("cargo:rerun-if-changed=third_party/fluidwin/motion_upstream.h");
        println!("cargo:rerun-if-changed=third_party/fluidwin/motion_renderer.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/fixture_crash.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/index_upstream.h");
        println!("cargo:rerun-if-changed=third_party/fluidwin/index_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/upstream");
        println!("cargo:rerun-if-changed=third_party/fluidwin/shell_menu_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/window_lease.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/audio_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/stack_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/snap_accessibility.h");
        println!("cargo:rerun-if-changed=third_party/fluidwin/control_access_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/sensors_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/static_capture_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/app_menu_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/shelf_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/wave_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/calendar_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/notifications_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/news_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/recycle_bridge.cpp");
        println!("cargo:rerun-if-changed=third_party/fluidwin/media_bridge.cpp");
        cc::Build::new()
            .cpp(true)
            .debug(true)
            .flag("/std:c++17")
            .flag("/utf-8")
            .flag("/EHsc")
            .define("NOMINMAX", None)
            .define("WIN32_LEAN_AND_MEAN", None)
            .file("third_party/fluidwin/motion_renderer.cpp")
            .file("third_party/fluidwin/fixture_crash.cpp")
            .file("third_party/fluidwin/index_bridge.cpp")
            .file("third_party/fluidwin/shell_menu_bridge.cpp")
            .file("third_party/fluidwin/window_lease.cpp")
            .file("third_party/fluidwin/audio_bridge.cpp")
            .file("third_party/fluidwin/stack_bridge.cpp")
            .file("third_party/fluidwin/control_access_bridge.cpp")
            .file("third_party/fluidwin/sensors_bridge.cpp")
            .file("third_party/fluidwin/static_capture_bridge.cpp")
            .file("third_party/fluidwin/app_menu_bridge.cpp")
            .file("third_party/fluidwin/shelf_bridge.cpp")
            .file("third_party/fluidwin/wave_bridge.cpp")
            .file("third_party/fluidwin/calendar_bridge.cpp")
            .file("third_party/fluidwin/notifications_bridge.cpp")
            .file("third_party/fluidwin/news_bridge.cpp")
            .file("third_party/fluidwin/recycle_bridge.cpp")
            .file("third_party/fluidwin/media_bridge.cpp")
            .compile("tlauncher_fluidwin_motion");
        println!("cargo:rustc-link-lib=d3dcompiler");
        println!("cargo:rustc-link-lib=uuid");
        println!("cargo:rustc-link-lib=ole32");
        println!("cargo:rustc-link-lib=shell32");
        println!("cargo:rustc-link-lib=winmm");
        println!("cargo:rustc-link-lib=windowscodecs");
        println!("cargo:rustc-link-lib=shcore");
        for lib in ["d2d1","dwrite","dwmapi","oleaut32","runtimeobject","shlwapi","advapi32","version","dxva2","propsys"] {
            println!("cargo:rustc-link-lib={lib}");
        }
        let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let manifest_path = manifest_dir.join("app.manifest");
        // Both the shipping executable and native test harness need the
        // same Windows 8+ layered-child and PerMonitorV2 declarations.
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest_path.display());

        let rc_exe = find_rc_exe().expect("Không tìm thấy rc.exe của Windows SDK để nhúng assets/app.ico");
        let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
        if std::env::var("GITHUB_ACTIONS").as_deref()==Ok("true"){
            println!("cargo:rustc-link-arg=/MAP:{}",out_dir.join("native-symbols.map").display());
        }
        let res_out = out_dir.join("app.res");
        let rc_in = manifest_dir.join("app.rc");
        let status = Command::new(&rc_exe)
            .current_dir(&manifest_dir)
            .arg("/nologo")
            .arg("/fo")
            .arg(&res_out)
            .arg(&rc_in)
            .status()
            .expect("Không chạy được rc.exe để biên dịch app.rc");
        assert!(status.success(), "rc.exe biên dịch app.rc thất bại (mã thoát: {:?})", status.code());
        println!("cargo:rustc-link-arg-bins={}", res_out.display());
    } else if target.contains("windows") && target.contains("gnu") {
        build_gnu_resources();
    }
}

/// Máy không cài được MSVC Build Tools (cần quyền quản trị): bộ công cụ GNU không có /MANIFEST,
/// nên dựng tài nguyên manifest bằng Rust (embed-manifest) và biên dịch app.rc bằng windres của MinGW.
fn build_gnu_resources() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    embed_manifest::embed_manifest_file(manifest_dir.join("app.manifest")).expect("Không nhúng được app.manifest vào exe (bản build GNU)");
    let res_out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("app-res.o");
    let built = Command::new("windres")
        .current_dir(&manifest_dir)
        .args(["app.rc", "-O", "coff", "-o"])
        .arg(&res_out)
        .status()
        .is_ok_and(|s| s.success());
    if built {
        println!("cargo:rustc-link-arg-bins={}", res_out.display());
    } else {
        println!("cargo:warning=Không có windres (MinGW) trong PATH: exe sẽ thiếu biểu tượng và VERSIONINFO");
    }
}
