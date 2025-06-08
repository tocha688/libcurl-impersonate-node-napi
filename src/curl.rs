use napi::{Error, Result, Status};
use napi_derive::napi;
use std::cell::UnsafeCell;
use std::os::raw::{c_char, c_int, c_long, c_void};

use crate::constants::CurlImpersonate;
use crate::{
  constants::{CurlInfo, CurlOpt},
  loader::{napi_load_library, CurlFunctions, CurlHandle},
};

// 简单的内存写入回调
extern "C" fn write_data(
  ptr: *mut c_char,
  size: usize,
  nmemb: usize,
  stream: *mut c_void,
) -> usize {
  let real_size = size * nmemb;
  if !ptr.is_null() && !stream.is_null() && real_size > 0 {
    let data = unsafe { std::slice::from_raw_parts(ptr as *const u8, real_size) };
    let buffer = unsafe { &mut *(stream as *mut Vec<u8>) };
    buffer.extend_from_slice(data);
  }
  real_size
}

#[napi]
pub struct Curl {
  handle: CurlHandle,
  lib: &'static CurlFunctions,
  header_buffer: UnsafeCell<Vec<u8>>,
  content_buffer: UnsafeCell<Vec<u8>>,
}

// 手动实现 Clone
impl Clone for Curl {
  fn clone(&self) -> Self {
    unsafe {
      let lib = self.lib;
      let handle = (lib.easy_init)();

      if handle.is_null() {
        panic!("Failed to initialize curl handle during clone");
      }

      // 复制当前的数据
      let header_data = (*self.header_buffer.get()).clone();
      let content_data = (*self.content_buffer.get()).clone();

      Curl {
        lib,
        handle,
        header_buffer: UnsafeCell::new(header_data),
        content_buffer: UnsafeCell::new(content_data),
      }
    }
  }
}

// UnsafeCell 需要手动实现 Send 和 Sync
unsafe impl Send for Curl {}
unsafe impl Sync for Curl {}

#[napi]
impl Curl {
  #[napi(constructor)]
  pub fn new() -> napi::Result<Self> {
    unsafe {
      let lib = napi_load_library()?;

      let handle = (lib.easy_init)();
      if handle.is_null() {
        return Err(Error::new(
          Status::GenericFailure,
          "Failed to initialize curl handle",
        ));
      }

      let curl = Curl {
        lib,
        handle,
        header_buffer: UnsafeCell::new(Vec::new()),
        content_buffer: UnsafeCell::new(Vec::new()),
      };

      Ok(curl)
    }
  }

  /// 初始化数据回调
  #[napi]
  pub fn init(&self) {
    unsafe {
      (*self.header_buffer.get()).clear();
      (*self.content_buffer.get()).clear();
      // 设置写入函数
      (self.lib.easy_setopt)(
        self.handle,
        CurlOpt::WriteFunction as c_int,
        write_data as *const c_void,
      );

      // 设置响应体数据存储
      (self.lib.easy_setopt)(
        self.handle,
        CurlOpt::WriteData as c_int,
        self.content_buffer.get() as *mut c_void,
      );

      // 设置头部写入函数
      (self.lib.easy_setopt)(
        self.handle,
        CurlOpt::HeaderFunction as c_int,
        write_data as *const c_void,
      );

      // 设置响应头数据存储
      (self.lib.easy_setopt)(
        self.handle,
        CurlOpt::HeaderData as c_int,
        self.header_buffer.get() as *mut c_void,
      );
    }
  }

  /// 设置字符串选项
  #[napi]
  pub fn set_opt_string(&self, option: CurlOpt, value: String) -> i32 {
    let c_str = std::ffi::CString::new(value).unwrap();
    unsafe {
      (self.lib.easy_setopt)(
        self.handle,
        option as c_int,
        c_str.as_ptr() as *const c_void,
      )
    }
  }

  /// 设置长整型选项
  #[napi]
  pub fn set_opt_long(&self, option: CurlOpt, value: i64) -> i32 {
    unsafe { (self.lib.easy_setopt)(self.handle, option as c_int, value as *const c_void) }
  }

  /// 设置boolean
  #[napi]
  pub fn set_opt_bool(&self, option: CurlOpt, value: bool) -> i32 {
    unsafe {
      (self.lib.easy_setopt)(
        self.handle,
        option as c_int,
        if value { 1 } else { 0 } as *const c_void,
      )
    }
  }

  /// 传入bytes
  #[napi]
  pub fn set_opt_bytes(&self, option: CurlOpt, body: Vec<u8>) -> i32 {
    unsafe { (self.lib.easy_setopt)(self.handle, option as c_int, body.as_ptr() as *const c_void) }
  }

  /// 获取响应码
  #[napi]
  pub fn get_info_number(&self, option: CurlInfo) -> Result<i64> {
    let mut response_code: c_long = 0;
    let result = unsafe {
      (self.lib.easy_getinfo)(
        self.handle,
        option as c_int,
        &mut response_code as *mut _ as *mut c_void,
      )
    };
    if result == 0 {
      Ok(response_code as i64)
    } else {
      Err(Error::new(
        Status::GenericFailure,
        format!("curl_easy_getinfo failed with code: {}", result),
      ))
    }
  }

  /// 获取字符串信息
  #[napi]
  pub fn get_info_string(&self, option: CurlInfo) -> Result<String> {
    let mut url_ptr: *mut c_char = std::ptr::null_mut();
    let result = unsafe {
      (self.lib.easy_getinfo)(
        self.handle,
        option as c_int,
        &mut url_ptr as *mut _ as *mut c_void,
      )
    };
    if result == 0 && !url_ptr.is_null() {
      let cstr = unsafe { std::ffi::CStr::from_ptr(url_ptr) };
      Ok(cstr.to_string_lossy().to_string())
    } else {
      Err(Error::new(
        Status::GenericFailure,
        format!("curl_easy_getinfo failed with code: {}", result),
      ))
    }
  }

  /// 模拟浏览器
  #[napi]
  pub fn impersonate(&self, target: String, default_headers: Option<bool>) -> i32 {
    let target_cstr = std::ffi::CString::new(target).unwrap();
    let use_default_headers = default_headers.unwrap_or(true);

    unsafe {
      (self.lib.easy_impersonate)(
        self.handle,
        target_cstr.as_ptr(),
        if use_default_headers { 1 } else { 0 },
      )
    }
  }

  /// 获取错误信息字符串
  #[napi]
  pub fn error(&self, code: i32) -> String {
    unsafe {
      let ptr = (self.lib.easy_strerror)(code);
      let cstr = std::ffi::CStr::from_ptr(ptr);
      cstr.to_string_lossy().to_string()
    }
  }

  /// 获取curlID
  #[napi]
  pub fn id(&self) -> String {
    format!("0x{:x}", self.handle as usize)
  }

  /// 清理 curl handle
  #[napi]
  pub fn close(&self) {
    unsafe {
      (self.lib.easy_cleanup)(self.handle);
    }
  }

  /// 重置 curl
  #[napi]
  pub fn reset(&self) {
    unsafe {
      (*self.header_buffer.get()).clear();
      (*self.content_buffer.get()).clear();
      (self.lib.easy_reset)(self.handle);
    }
  }

  /// 执行 curl 请求
  #[napi]
  pub fn perform(&self) -> i32 {
    // 确保数据回调已初始化
    self.init();
    unsafe { (self.lib.easy_perform)(self.handle) }
  }

  /// 获取响应头数据
  #[napi]
  pub fn get_headers(&self) -> Vec<u8> {
    unsafe { (*self.header_buffer.get()).clone() }
  }

  /// 获取响应体数据
  #[napi]
  pub fn get_body(&self) -> Vec<u8> {
    unsafe { (*self.content_buffer.get()).clone() }
  }

  /// 获取 curl handle（内部使用）
  pub fn get_handle(&self) -> CurlHandle {
    self.handle
  }
}

// 为了安全，实现 Drop trait 来确保资源正确清理
impl Drop for Curl {
  fn drop(&mut self) {
    if !self.handle.is_null() {
      self.close();
    }
  }
}
