use napi_derive::napi;
use std::env;
use std::path::{Path, PathBuf};

pub fn get_ptr_address<T>(ptr: *const T) -> String {
    format!("0x{:x}", ptr as usize)
}

#[napi]
pub fn get_default_dir_name() -> String {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "arm64" => "arm64",
        "arm" => "arm-linux-gnueabihf",
        "riscv64" => "riscv64",
        "i386" => "i386",
        "ia32" => "i686",
        other => other,
    };

    let platform = match std::env::consts::OS {
        "linux" => "linux-gnu",
        "macos" => "macos",
        "windows" => "win32",
        other => other,
    };

    format!("{}-{}", arch, platform)
}

#[napi]
pub fn get_default_lib_path() -> String {
    // 相当于 __dirname/../libs
    // 这里用当前可执行文件路径的父目录再上一级
    // let exe_path = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let base_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("libs");
    // let base_dir = exe_path.parent()
    //     .and_then(|p| p.parent())
    //     .map(|p| p.join("libs"))
    //     .unwrap_or_else(|| PathBuf::from("libs"));

    let dir_name = get_default_dir_name();

    let lib_name = match std::env::consts::OS {
        "windows" => Path::new("bin").join("libcurl.dll"),
        "macos" => Path::new("libcurl-impersonate.dylib").to_path_buf(),
        "linux" => Path::new("libcurl-impersonate.so").to_path_buf(),
        _ => Path::new("libcurl-impersonate.so").to_path_buf(),
    };

    base_dir.join(dir_name).join(lib_name).to_string_lossy().to_string()
}