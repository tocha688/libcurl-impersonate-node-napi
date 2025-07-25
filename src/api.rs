use napi::{Error, Status};
use napi_derive::napi;
use std::ffi::c_long;

use crate::loader::napi_load_library;

/// 全局初始化
#[napi]
pub fn global_init(flags: i64) -> napi::Result<i32> {
  unsafe {
    let lib = napi_load_library()?;
    Ok((lib.global_init)(flags as c_long))
  }
}

/// 全局清理
#[napi]
pub fn global_cleanup() -> napi::Result<()> {
  unsafe {
    let lib = napi_load_library()?;
    (lib.global_cleanup)();
    Ok(())
  }
}

/// 获取 libcurl 版本信息
#[napi]
pub fn get_version() -> napi::Result<String> {
  let lib = napi_load_library()?;
  unsafe {
    let version_ptr = (lib.version)(); // 修正：curl_version() 不需要参数
    if version_ptr.is_null() {
      return Err(Error::new(Status::GenericFailure, "Failed to get version"));
    }
    let version_cstr = std::ffi::CStr::from_ptr(version_ptr);
    Ok(version_cstr.to_string_lossy().to_string())
  }
}

#[napi]
pub fn curl_easy_error(code: i32) -> String {
  let lib = napi_load_library().expect("Failed to load libcurl library");
  unsafe {
    let ptr = (lib.easy_strerror)(code);
    let cstr = std::ffi::CStr::from_ptr(ptr);
    cstr.to_string_lossy().to_string()
  }
}

#[napi]
pub fn curl_multi_error(code: i32) -> String {
  let lib = napi_load_library().expect("Failed to load libcurl library");
  unsafe {
    let ptr = (lib.multi_strerror)(code);
    let cstr = std::ffi::CStr::from_ptr(ptr);
    cstr.to_string_lossy().to_string()
  }
}
