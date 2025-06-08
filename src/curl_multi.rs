use crate::curl::Curl;
use crate::constants::CurlMOpt;
use crate::loader::{napi_load_library, CurlFunctions, CurlHandle, CurlMultiHandle};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::os::raw::{c_int, c_void};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[napi]
pub struct CurlMulti {
  handle: CurlMultiHandle,
  lib: &'static CurlFunctions,
  active_handles: Vec<CurlHandle>,
}

// 实现Send和Sync trait
unsafe impl Send for CurlMulti {}
unsafe impl Sync for CurlMulti {}

#[napi]
impl CurlMulti {
  #[napi(constructor)]
  pub fn new() -> Result<Self> {
    let functions = napi_load_library()?;
    let handle = unsafe { (functions.multi_init)() };

    if handle.is_null() {
      return Err(Error::from_reason("Failed to initialize curl multi handle"));
    }

    Ok(CurlMulti {
      handle,
      lib: functions,
      active_handles: Vec::new(),
    })
  }

  //------------setopt-------------------

  /// 设置字符串选项
  #[napi]
  pub fn set_opt_string(&self, option: CurlMOpt, value: String) -> i32 {
    let c_str = std::ffi::CString::new(value).unwrap();
    unsafe {
      (self.lib.multi_setopt)(
        self.handle,
        option as c_int,
        c_str.as_ptr() as *const c_void,
      )
    }
  }

  /// 设置长整型选项
  #[napi]
  pub fn set_opt_long(&self, option: CurlMOpt, value: i64) -> i32 {
    unsafe { (self.lib.multi_setopt)(self.handle, option as c_int, value as *const c_void) }
  }

  /// 设置boolean
  #[napi]
  pub fn set_opt_bool(&self, option: CurlMOpt, value: bool) -> i32 {
    unsafe {
      (self.lib.multi_setopt)(
        self.handle,
        option as c_int,
        if value { 1 } else { 0 } as *const c_void,
      )
    }
  }

  /// 添加 curl handle 到 multi handle
  #[napi]
  pub fn add_handle(&mut self, curl: &Curl) -> Result<i32> {
    let curl_handle = curl.get_handle();
    let result = unsafe { (self.lib.multi_add_handle)(self.handle, curl_handle) };
    
    if result == 0 {
      self.active_handles.push(curl_handle);
    }
    
    Ok(result)
  }

  /// 从 multi handle 移除 curl handle
  #[napi]
  pub fn remove_handle(&mut self, curl: &Curl) -> Result<i32> {
    let curl_handle = curl.get_handle();
    let result = unsafe { (self.lib.multi_remove_handle)(self.handle, curl_handle) };
    
    if result == 0 {
      self.active_handles.retain(|&h| h != curl_handle);
    }
    
    Ok(result)
  }

  /// 异步执行并等待返回
  #[napi]
  pub async fn perform(&mut self, curl: &Curl) -> Result<i32> {
    // 添加 handle 到 multi
    self.add_handle(curl)?;
    
    let mut running_handles = 0i32;
    let start_time = Instant::now();
    let timeout = Duration::from_secs(30); // 30秒超时

    loop {
      // 执行 curl multi perform
      let result = unsafe {
        (self.lib.multi_perform)(self.handle, &mut running_handles as *mut c_int)
      };

      if result != 0 && result != -1 { // CURLM_CALL_MULTI_PERFORM = -1
        self.remove_handle(curl)?;
        return Err(Error::from_reason(format!("curl_multi_perform failed with code: {}", result)));
      }

      // 检查是否所有请求都完成了
      if running_handles == 0 {
        break;
      }

      // 检查超时
      if start_time.elapsed() > timeout {
        self.remove_handle(curl)?;
        return Err(Error::from_reason("Request timeout"));
      }

      // 等待 socket 活动或超时
      let mut timeout_ms = 1000i64; // 默认1秒
      
      // 获取 curl 推荐的超时时间
      unsafe {
        (self.lib.multi_timeout)(self.handle, &mut timeout_ms as *mut i64);
      }

      if timeout_ms < 0 {
        timeout_ms = 1000; // 如果没有推荐时间，使用1秒
      } else if timeout_ms == 0 {
        continue; // 立即继续
      } else if timeout_ms > 1000 {
        timeout_ms = 1000; // 最多等待1秒
      }

      // 异步等待
      sleep(Duration::from_millis(timeout_ms as u64)).await;
    }

    // 检查消息队列，获取请求结果
    let mut msgs_left = 0i32;
    loop {
      let msg = unsafe {
        (self.lib.multi_info_read)(self.handle, &mut msgs_left as *mut c_int)
      };

      if msg.is_null() {
        break;
      }

      let curl_msg = unsafe { &*msg };
      
      // 检查消息类型和结果
      if curl_msg.msg == 1 { // CURLMSG_DONE = 1
        let result_code = curl_msg.data.result;
        
        // 移除完成的 handle
        self.remove_handle(curl)?;
        
        return Ok(result_code);
      }
    }

    // 如果没有找到消息，移除 handle 并返回成功
    self.remove_handle(curl)?;
    Ok(0)
  }

  /// 执行多个 curl 请求
  #[napi]
  pub async fn perform_all(&mut self, curls: Vec<&Curl>) -> Result<Vec<i32>> {
    // 添加所有 handles
    for curl in &curls {
      self.add_handle(curl)?;
    }

    let mut running_handles = 0i32;
    let mut results = Vec::new();
    let start_time = Instant::now();
    let timeout = Duration::from_secs(60); // 60秒超时

    loop {
      // 执行 curl multi perform
      let result = unsafe {
        (self.lib.multi_perform)(self.handle, &mut running_handles as *mut c_int)
      };

      if result != 0 && result != -1 { // CURLM_CALL_MULTI_PERFORM = -1
        // 清理所有 handles
        for curl in &curls {
          let _ = self.remove_handle(curl);
        }
        return Err(Error::from_reason(format!("curl_multi_perform failed with code: {}", result)));
      }

      // 检查消息队列
      let mut msgs_left = 0i32;
      loop {
        let msg = unsafe {
          (self.lib.multi_info_read)(self.handle, &mut msgs_left as *mut c_int)
        };

        if msg.is_null() {
          break;
        }

        let curl_msg = unsafe { &*msg };
        
        if curl_msg.msg == 1 { // CURLMSG_DONE = 1
          let result_code = curl_msg.data.result;
          results.push(result_code);
          
          // 从 active_handles 中移除
          let easy_handle = curl_msg.easy_handle;
          self.active_handles.retain(|&h| h != easy_handle);
          
          // 从 multi handle 中移除
          unsafe {
            (self.lib.multi_remove_handle)(self.handle, easy_handle);
          }
        }
      }

      // 检查是否所有请求都完成了
      if running_handles == 0 || results.len() == curls.len() {
        break;
      }

      // 检查超时
      if start_time.elapsed() > timeout {
        // 清理剩余的 handles
        for curl in &curls {
          let _ = self.remove_handle(curl);
        }
        return Err(Error::from_reason("Request timeout"));
      }

      // 等待 socket 活动
      let mut timeout_ms = 1000i64;
      unsafe {
        (self.lib.multi_timeout)(self.handle, &mut timeout_ms as *mut i64);
      }

      if timeout_ms < 0 {
        timeout_ms = 100;
      } else if timeout_ms == 0 {
        continue;
      } else if timeout_ms > 1000 {
        timeout_ms = 1000;
      }

      sleep(Duration::from_millis(timeout_ms as u64)).await;
    }

    Ok(results)
  }

  /// 获取当前活动的 handle 数量
  #[napi]
  pub fn get_active_count(&self) -> u32 {
    self.active_handles.len() as u32
  }

  /// 清除所有活动的 handles
  #[napi]
  pub fn clear_handles(&mut self) -> Result<()> {
    for &handle in &self.active_handles {
      unsafe {
        (self.lib.multi_remove_handle)(self.handle, handle);
      }
    }
    self.active_handles.clear();
    Ok(())
  }
}

impl Drop for CurlMulti {
  fn drop(&mut self) {
    if !self.handle.is_null() {
      unsafe {
        // 清理所有活动的 handles
        for &handle in &self.active_handles {
          (self.lib.multi_remove_handle)(self.handle, handle);
        }
        self.active_handles.clear();
        (self.lib.multi_cleanup)(self.handle);
      }
    }
  }
}

// 需要添加的结构体定义（如果在其他地方没有定义）
#[repr(C)]
struct CurlMsg {
  msg: c_int,
  easy_handle: CurlHandle,
  data: CurlMsgData,
}

#[repr(C)]
union CurlMsgData {
  whatever: *mut c_void,
  result: c_int,
}
