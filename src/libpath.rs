use std::env;
use std::sync::RwLock;
use napi_derive::napi;
use once_cell::sync::Lazy;

const CURL_IMPERSONATE_VERSION: &str = "v1.0.0";
const BASE_URL: &str = "https://github.com/lexiforest/@tocha688/libcurl/releases/download";

#[derive(Debug, Clone)]
pub struct PlatformInfo {
    pub arch: String,
    pub os: String,
    pub variant: Option<String>,
}

impl PlatformInfo {
    pub fn detect() -> Self {
        let arch = match env::consts::ARCH {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            "arm" => "arm",
            "x86" => "i386",
            "riscv64" => "riscv64",
            other => other,
        }.to_string();

        let (os, variant) = match env::consts::OS {
            "linux" => {
                // 检测是否是 musl
                let is_musl = std::process::Command::new("ldd")
                    .arg("--version")
                    .output()
                    .map(|output| String::from_utf8_lossy(&output.stderr).contains("musl"))
                    .unwrap_or(false);
                
                if is_musl {
                    ("linux-musl".to_string(), None)
                } else if arch == "arm" {
                    ("linux-gnueabihf".to_string(), None)
                } else {
                    ("linux-gnu".to_string(), None)
                }
            },
            "macos" => ("macos".to_string(), None),
            "windows" => ("win32".to_string(), None),
            other => (other.to_string(), None),
        };

        PlatformInfo { arch, os, variant }
    }

    pub fn get_download_filename(&self, is_libcurl: bool) -> String {
        let prefix = if is_libcurl { "lib@tocha688/libcurl" } else { "@tocha688/libcurl" };
        
        // 特殊处理 Windows 架构映射
        let arch = if self.os.contains("win32") {
            match self.arch.as_str() {
                "x86" | "i386" => "i686",
                other => other,
            }
        } else {
            &self.arch
        };

        format!("{}-{}-{}-{}.tar.gz", prefix, CURL_IMPERSONATE_VERSION, arch, self.os)
    }

    pub fn get_download_url(&self, is_libcurl: bool) -> String {
        let filename = self.get_download_filename(is_libcurl);
        format!("{}/{}/{}", BASE_URL, CURL_IMPERSONATE_VERSION, filename)
    }
    
}

// 使用 RwLock 替代 mutable static，更安全
static LIB_PATH: Lazy<RwLock<Option<String>>> = Lazy::new(|| RwLock::new(None));

#[napi]
pub fn set_lib_path(path: String) {
    if let Ok(mut lib_path) = LIB_PATH.write() {
        *lib_path = Some(path);
    }
}

#[napi]
pub fn get_lib_path() -> Option<String> {
    LIB_PATH.read().ok()?.clone()
}

