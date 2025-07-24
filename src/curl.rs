use napi::bindgen_prelude::Buffer;
use napi::{Error, Result, Status};
use napi_derive::napi;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::os::raw::{c_char, c_int, c_long, c_void};

use crate::loader::CurlSlistNode;
use crate::log_info;
use crate::utils::get_ptr_address;
use crate::{
  constants::{CurlInfo, CurlOpt},
  loader::{napi_load_library, CurlFunctions, CurlHandle, CurlSlist},
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
  pub closed: bool,
  handle: CurlHandle,
  lib: &'static CurlFunctions,
  header_buffer: UnsafeCell<Vec<u8>>,
  content_buffer: UnsafeCell<Vec<u8>>,
  headers_list: UnsafeCell<Option<CurlSlist>>, // 添加 headers 列表管理
}

// 手动实现 Clone - 修正版本
// impl Clone for Curl {
//   fn clone(&self) -> Self {
//     unsafe {
//       let lib = self.lib;

//       // 创建新的 curl handle，而不是复制指针
//       let new_handle = (lib.easy_init)();
//       if new_handle.is_null() {
//         panic!("Failed to create new curl handle in clone");
//       }

//       // 复制当前的数据
//       let header_data = (*self.header_buffer.get()).clone();
//       let content_data = (*self.content_buffer.get()).clone();

//       let new_curl = Curl {
//         closed: false,
//         lib,
//         handle: new_handle,
//         header_buffer: UnsafeCell::new(header_data),
//         content_buffer: UnsafeCell::new(content_data),
//         headers_list: UnsafeCell::new(None), // 新实例不复制 headers 列表
//       };

//       new_curl
//     }
//   }
// }

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
        closed: false,
        lib,
        handle,
        header_buffer: UnsafeCell::new(Vec::new()),
        content_buffer: UnsafeCell::new(Vec::new()),
        headers_list: UnsafeCell::new(None), // 初始化 headers 列表
      };

      Ok(curl)
    }
  }

  /// 初始化数据回调
  #[napi]
  pub fn init(&self) {
    log_info!("Curl", "Initializing curl data callbacks");
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

      // 设置响应头数据存储
      (self.lib.easy_setopt)(
        self.handle,
        CurlOpt::HeaderData as c_int,
        self.header_buffer.get() as *mut c_void,
      );

      // *** 重要：启用 cookie 引擎 ***
      (self.lib.easy_setopt)(
        self.handle,
        CurlOpt::CookieJar as c_int,
        std::ptr::null::<c_void>(), // 使用内存中的 cookie jar
      );
    }
  }

  pub fn check_close(&self) -> Result<()> {
    // Check if the handle is valid
    if self.closed {
      return Err(Error::from_reason("Curl instance is closed"));
    }
    if self.handle.is_null() {
      return Err(Error::from_reason("Curl handle is null"));
    }
    Ok(())
  }

  /// 设置单个 HTTP 头
  #[napi]
  pub fn add_header(&self, name: String, value: String) -> Result<()> {
    let header = format!("{}: {}", name, value);
    self.add_header_raw(header)
  }

  /// 设置原始 HTTP 头
  #[napi]
  pub fn add_header_raw(&self, header: String) -> Result<()> {
    self.check_close()?;
    let header_cstr = std::ffi::CString::new(header)
      .map_err(|_| Error::new(Status::InvalidArg, "Invalid header string"))?;

    unsafe {
      let headers_list_ptr = self.headers_list.get();
      let current_list = (*headers_list_ptr).unwrap_or(std::ptr::null_mut());

      // 添加新的 header 到列表
      let new_list = (self.lib.slist_append)(current_list, header_cstr.as_ptr());
      if new_list.is_null() {
        return Err(Error::new(
          Status::GenericFailure,
          "Failed to append header",
        ));
      }

      (*headers_list_ptr) = Some(new_list);

      self.set_opt(CurlOpt::HttpHeader, new_list as *const c_void)
    }
  }

  #[napi]
  pub fn set_headers(&self, headers: HashMap<String, String>) -> Result<()> {
    let _ = self.clear_headers();
    // 逐个添加 headers
    for (name, value) in headers {
      self.add_header(name, value)?;
    }
    Ok(())
  }

  #[napi]
  pub fn set_headers_raw(&self, headers: Vec<String>) -> Result<()> {
    let _ = self.clear_headers();
    // 逐个添加 headers
    for header in headers {
      self.add_header_raw(header.clone())?;
    }
    Ok(())
  }

  /// 清理所有 HTTP 头
  #[napi]
  pub fn clear_headers(&self) -> Result<()> {
    self.check_close()?;
    unsafe {
      let headers_list_ptr = self.headers_list.get();
      if let Some(headers_list) = *headers_list_ptr {
        if !headers_list.is_null() {
          (self.lib.slist_free_all)(headers_list);
        }
      }
      (*headers_list_ptr) = None;

      // 重置 curl 的 headers
      (self.lib.easy_setopt)(
        self.handle,
        CurlOpt::HttpHeader as c_int,
        std::ptr::null::<c_void>(),
      );
    }
    Ok(())
  }

  pub fn set_opt(&self, option: CurlOpt, value: *const c_void) -> Result<()> {
    self.check_close()?;
    log_info!(
      "Curl",
      "Setting option: {:?} with value: {:?}",
      option,
      value
    );
    self.result(unsafe { (self.lib.easy_setopt)(self.handle, option as c_int, value) })
  }

  /// 设置字符串选项
  #[napi]
  pub fn set_opt_string(&self, option: CurlOpt, value: String) -> Result<()> {
    let c_str = std::ffi::CString::new(value).unwrap();
    self.set_opt(option, c_str.as_ptr() as *const c_void)
  }

  /// 设置长整型选项
  #[napi]
  pub fn set_opt_long(&self, option: CurlOpt, value: i64) -> Result<()> {
    self.set_opt(option, value as *const c_void)
  }

  /// 设置boolean
  #[napi]
  pub fn set_opt_bool(&self, option: CurlOpt, value: bool) -> Result<()> {
    self.set_opt(option, if value { 1 } else { 0 } as *const c_void)
  }

  /// 传入bytes
  #[napi]
  pub fn set_opt_bytes(&self, option: CurlOpt, body: Vec<u8>) -> Result<()> {
    self.set_opt(option, body.as_ptr() as *const c_void)
  }

  #[napi]
  pub fn set_opt_buffer(&self, option: CurlOpt, body: Buffer) -> Result<()> {
    self.set_opt(option, body.as_ptr() as *const c_void)
  }

  #[napi]
  pub fn set_opt_str_list(&self, option: CurlOpt, arrays: Vec<String>) -> Result<()> {
    self.check_close()?;
     self.set_opt(option, arrays.as_ptr() as *const c_void)
  }

  fn result(&self, code: i32) -> Result<()> {
    if code != 0 {
      Err(Error::new(
        Status::GenericFailure,
        format!("failed with code: {} message:{}", code, self.error(code)),
      ))
    } else {
      Ok(())
    }
  }

  pub fn get_info(&self, info: CurlInfo, value: *mut c_void) -> Result<()> {
    self.check_close()?;
    log_info!("Curl", "{:?}Get info: {:?} ", self.id(), info);
    self.result(unsafe { (self.lib.easy_getinfo)(self.handle, info as c_int, value) })
  }

  /// 获取响应码
  #[napi]
  pub fn get_info_number(&self, option: CurlInfo) -> Result<i64> {
    let mut response_code: c_long = 0;
    self.get_info(option, &mut response_code as *mut _ as *mut c_void)?;
    Ok(response_code as i64)
  }

  /// 获取字符串信息
  #[napi]
  pub fn get_info_string(&self, option: CurlInfo) -> Result<String> {
    let mut url_ptr: *mut c_char = std::ptr::null_mut();
    self.get_info(option, &mut url_ptr as *mut _ as *mut c_void)?;
    let cstr = unsafe { std::ffi::CStr::from_ptr(url_ptr) };
    Ok(cstr.to_string_lossy().to_string())
  }

  /// 模拟浏览器
  #[napi]
  pub fn impersonate(&self, target: String, default_headers: Option<bool>) -> Result<()> {
    self.check_close()?;
    let target_cstr = std::ffi::CString::new(target.clone()).unwrap();
    let use_default_headers = default_headers.unwrap_or(true);
    log_info!(
      "Curl",
      "Impersonating as: {} with default headers: {}",
      target,
      use_default_headers
    );

    self.result(unsafe {
      (self.lib.easy_impersonate)(
        self.handle,
        target_cstr.as_ptr(),
        if use_default_headers { 1 } else { 0 },
      )
    })
  }

  /// 获取错误信息字符串
  #[napi]
  pub fn error(&self, code: i32) -> String {
    log_info!("Curl", "error {}", code);
    unsafe {
      let ptr = (self.lib.easy_strerror)(code);
      let cstr = std::ffi::CStr::from_ptr(ptr);
      cstr.to_string_lossy().to_string()
    }
  }

  /// 获取curlID
  #[napi]
  pub fn id(&self) -> String {
    get_ptr_address(self.handle)
  }

  /// 获取 curl handle（内部使用）- 添加安全检查
  pub fn get_handle(&self) -> CurlHandle {
    if self.handle.is_null() {
      println!("Warning: curl handle is null!");
    }
    self.handle
  }

  /// 清理 curl handle
  #[napi]
  pub fn close(&mut self) {
    if self.closed {
      return;
    }
    self.closed = true;
    if self.handle.is_null() {
      return;
    }

    // 先清理 headers
    let _ = self.clear_headers();

    log_info!("Curl", "easy_cleanup {:?}", self.id());
    unsafe {
      (self.lib.easy_cleanup)(self.handle);
    }
  }

  /// 重置 curl
  #[napi]
  pub fn reset(&self) -> Result<()> {
    self.check_close()?;
    log_info!("Curl", "easy_reset");
    unsafe {
      (*self.header_buffer.get()).clear();
      (*self.content_buffer.get()).clear();

      // 清理 headers
      let _ = self.clear_headers();

      (self.lib.easy_reset)(self.handle);
    }
    Ok(())
  }

  /// 执行 curl 请求
  #[napi]
  pub fn perform(&self) -> Result<()> {
    // 确保数据回调已初始化
    self.init();
    log_info!("Curl", "perform");
    self.result(unsafe { (self.lib.easy_perform)(self.handle) })
  }

  /// 获取响应头数据
  #[napi]
  pub fn get_resp_headers(&self) -> Buffer {
    unsafe { Buffer::from((*self.header_buffer.get()).clone()) }
  }

  /// 获取响应体数据
  #[napi]
  pub fn get_resp_body(&self) -> Buffer {
    unsafe { Buffer::from((*self.content_buffer.get()).clone()) }
  }

  /// 获取信息数组
  #[napi]
  pub fn get_info_list(&self, option: CurlInfo) -> Result<Vec<String>> {
    self.check_close()?;
    log_info!("Curl", "get_info_list {:?}", option);
    let mut cookie_list: CurlSlist = std::ptr::null_mut();
    self.get_info(option, &mut cookie_list as *mut _ as *mut c_void)?;
    let mut cookies = Vec::new();
    if !cookie_list.is_null() {
      unsafe {
        // 将指针转换为正确的结构体类型
        let mut current = cookie_list as *mut CurlSlistNode;

        // 遍历链表 - 就像 C 代码中的 while(each) 循环
        while !current.is_null() {
          let node = &*current;

          // 检查 data 指针是否有效
          if !node.data.is_null() {
            // 将 C 字符串转换为 Rust 字符串
            let cstr = std::ffi::CStr::from_ptr(node.data);
            if let Ok(cookie_str) = cstr.to_str() {
              cookies.push(cookie_str.to_string());
            }
          }

          // 移动到下一个节点 - 相当于 C 代码中的 each = each->next
          current = node.next;
        }

        // 释放 cookie 列表 - 相当于 C 代码中的 curl_slist_free_all(cookies)
        (self.lib.slist_free_all)(cookie_list);
      }
    }

    Ok(cookies)
  }
  /// 设置链表
  #[napi]
  pub fn set_opt_list(&self, option: CurlOpt, arrays: Vec<String>) -> Result<()> {
    self.check_close()?;
    log_info!("Curl", "set_opt_list {:?}", option);
    let mut list_ptr: CurlSlist = std::ptr::null_mut();

    for item in arrays {
      let item_cstr = std::ffi::CString::new(item)
        .map_err(|_| Error::new(Status::InvalidArg, "Invalid cookie string"))?;
      unsafe {
        list_ptr = (self.lib.slist_append)(list_ptr, item_cstr.as_ptr());
      }
    }

    if list_ptr.is_null() {
      return Err(Error::new(
        Status::GenericFailure,
        "Failed to create cookie list",
      ));
    }
    self.set_opt(option, list_ptr as *const c_void)?;
    // 释放链表
    unsafe {
      (self.lib.slist_free_all)(list_ptr);
    }

    Ok(())
  }

  /// 获取cookie列表
  #[napi]
  pub fn get_cookies(&self) -> Result<Vec<String>> {
    self.get_info_list(CurlInfo::CookieList)
  }

  /// 设置 cookie
  #[napi]
  pub fn set_cookies(&self, cookie: String) -> Result<()> {
    self.check_close()?;
    let cookie_cstr = std::ffi::CString::new(cookie)
      .map_err(|_| Error::new(Status::InvalidArg, "Invalid cookie string"))?;
    self.set_opt(CurlOpt::Cookie, cookie_cstr.as_ptr() as *const c_void)
  }

  #[napi]
  pub fn status(&self) -> Result<i32> {
    let result = self.get_info_number(CurlInfo::ResponseCode)?;
    Ok(result as i32)
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

