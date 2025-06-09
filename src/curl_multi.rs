use crate::constants::CurlMOpt;
use crate::curl::Curl;
use crate::loader::{napi_load_library, CurlFunctions, CurlHandle, CurlMultiHandle};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::os::raw::{c_int, c_long, c_void};
use std::sync::{Arc, Mutex};

// 存储回调的结构体
struct CurlCallback {
  success_callback: Option<napi::threadsafe_function::ThreadsafeFunction<CurlResult>>,
  error_callback: Option<napi::threadsafe_function::ThreadsafeFunction<String>>,
  curl: Arc<Curl>,
}

// 请求结果结构体
#[derive(Debug, Clone)]
pub struct CurlResult {
  pub result_code: i32,
  pub response_code: i64,
  pub headers: Vec<u8>,
  pub body: Vec<u8>,
}

#[napi]
pub struct CurlMulti {
  handle: CurlMultiHandle,
  lib: &'static CurlFunctions,
  curl_callbacks: Arc<Mutex<HashMap<String, CurlCallback>>>,
  running_count: UnsafeCell<i32>,
}

// Curl 常量
const CURLMSG_DONE: c_int = 1;
const CURL_SOCKET_TIMEOUT: c_int = -1;

// 消息结构体
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

// Socket 回调函数
extern "C" fn socket_function(
  _curl: CurlHandle,
  sockfd: c_int,
  what: c_int,
  userdata: *mut c_void,
  _socketp: *mut c_void,
) -> c_int {
  let id = format!("0x{:x}", _curl as usize);
  println!("socket_function: {} - {} - {}", id, sockfd, what);

  if userdata.is_null() {
    return 0;
  }

  let multi = unsafe { &*(userdata as *const CurlMulti) };
  multi.handle_socket_action(sockfd, what);

  0
}

// 定时器回调函数 - 修复参数类型
extern "C" fn timer_function(
  _curlm: CurlMultiHandle, // 修复：应该是 CurlMultiHandle
  timeout_ms: c_long,
  userdata: *mut c_void,
) -> c_int {
  let id = format!("0x{:x}", _curlm as usize);
  let cid = format!("0x{:x}", userdata as usize);
  println!("Timer function: {} - {} - {}", id, cid, timeout_ms);
  
  if userdata.is_null() {
    return 0;
  }

  let multi = unsafe { &*(userdata as *const CurlMulti) };
  multi.handle_timer_action_simple(timeout_ms);

  0
}

unsafe impl Send for CurlMulti {}
unsafe impl Sync for CurlMulti {}

#[napi]
impl CurlMulti {
  #[napi(constructor)]
  pub fn new() -> Result<Self> {
    let lib = napi_load_library()?;
    let handle = unsafe { (lib.multi_init)() };

    if handle.is_null() {
      return Err(Error::from_reason("Failed to initialize curl multi handle"));
    }

    let multi = CurlMulti {
      handle,
      lib,
      curl_callbacks: Arc::new(Mutex::new(HashMap::new())),
      running_count: UnsafeCell::new(0),
    };

    // 设置回调函数
    multi.setup_callbacks()?;

    Ok(multi)
  }

  /// 设置回调函数
  fn setup_callbacks(&self) -> Result<()> {
    unsafe {
      let ptr_value = self as *const Self as *const c_void as usize;
      println!("当前对象指针: 0x{:x} ({})", ptr_value, ptr_value);

      // 设置 socket 回调
      (self.lib.multi_setopt)(
        self.handle,
        CurlMOpt::SocketFunction as c_int,
        socket_function as *const c_void,
      );

      (self.lib.multi_setopt)(
        self.handle,
        CurlMOpt::SocketData as c_int,
        self as *const Self as *const c_void,
      );

      // 设置定时器回调
      (self.lib.multi_setopt)(
        self.handle,
        CurlMOpt::TimerFunction as c_int,
        timer_function as *const c_void,
      );

      (self.lib.multi_setopt)(
        self.handle,
        CurlMOpt::TimerData as c_int,
        self as *const Self as *const c_void,
      );
    }

    Ok(())
  }

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

  /// 设置boolean选项
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

  /// 异步执行单个请求
  #[napi]
  pub fn perform(
    &self,
    env: Env,
    curl: &Curl,
    success_callback: JsFunction,
    error_callback: Option<JsFunction>,
  ) -> Result<()> {
    curl.init();

    println!("加入curl id: {}", curl.id());
    
    // 创建 success threadsafe function
    let success_tsfn = env.create_threadsafe_function(&success_callback, 0, |ctx| {
      let result: CurlResult = ctx.value;
      let mut obj = ctx.env.create_object()?;
      obj.set_named_property("resultCode", ctx.env.create_int32(result.result_code)?)?;
      obj.set_named_property("responseCode", ctx.env.create_int64(result.response_code)?)?;
      obj.set_named_property(
        "headers",
        ctx.env.create_buffer_with_data(result.headers)?.into_raw(),
      )?;
      obj.set_named_property(
        "body",
        ctx.env.create_buffer_with_data(result.body)?.into_raw(),
      )?;
      Ok(vec![obj])
    })?;

    // 创建 error threadsafe function
    let error_tsfn = if let Some(error_cb) = error_callback {
      Some(env.create_threadsafe_function(&error_cb, 0, |ctx| {
        let error_msg: String = ctx.value;
        Ok(vec![ctx.env.create_string(&error_msg)?])
      })?)
    } else {
      None
    };

    // 添加到 multi handle
    let add_result = unsafe { (self.lib.multi_add_handle)(self.handle, curl.get_handle()) };

    if add_result != 0 {
      return Err(Error::from_reason(format!(
        "Failed to add handle: {}",
        add_result
      )));
    }

    // 存储回调信息
    if let Ok(mut curl_callbacks) = self.curl_callbacks.lock() {
      curl_callbacks.insert(
        curl.id(),
        CurlCallback {
          success_callback: Some(success_tsfn),
          error_callback: error_tsfn,
          curl: Arc::new(curl.clone()),
        },
      );
    }

    // 增加运行计数
    unsafe {
      *self.running_count.get() += 1;
    }

    println!("开始了");

    // 开始处理 - 先调用 multi_perform
    let mut running_handles = 0i32;
    unsafe {
      (self.lib.multi_perform)(self.handle, &mut running_handles);
    }
    
    println!("multi_perform result: {} running handles", running_handles);

    // 然后触发 socket action
    self.perform_socket_action(CURL_SOCKET_TIMEOUT, 0);
    
    Ok(())
  }

  fn perform_socket_action(&self, sockfd: c_int, ev_bitmask: c_int) {
    println!("perform_socket_action: sockfd={}, ev_bitmask={}", sockfd, ev_bitmask);
    
    let mut running_handles = 0i32;
    let result = unsafe {
      (self.lib.multi_socket_action)(
        self.handle,
        sockfd,
        ev_bitmask,
        &mut running_handles,
      )
    };
    
    println!("multi_socket_action result: {}, running_handles: {}", result, running_handles);
    
    // 检查是否有错误
    if result != 0 {
      println!("Error in multi_socket_action: {}", self.error(result as i64));
      return;
    }

    // 更新运行计数
    unsafe {
      *self.running_count.get() = running_handles;
    }

    // 关键：检查完成的消息
    self.check_finished_requests();
  }

  /// 检查完成的请求 - 这是必须的！
  fn check_finished_requests(&self) {
    println!("Checking for finished requests...");
    
    let mut msgs_left = 0i32;
    loop {
      let msg = unsafe { 
        (self.lib.multi_info_read)(self.handle, &mut msgs_left) 
      };

      if msg.is_null() {
        break;
      }

      let curl_msg = unsafe { &*(msg as *const CurlMsg) };
      
      if curl_msg.msg == CURLMSG_DONE {
        let easy_handle = curl_msg.easy_handle;
        let result_code = unsafe { curl_msg.data.result };
        
        println!("Request completed: handle={:p}, result={}", easy_handle, result_code);
        
        // 处理完成的请求
        self.handle_completed_request(easy_handle, result_code);
      }
    }
  }

  /// 处理完成的请求
  fn handle_completed_request(&self, easy_handle: CurlHandle, result_code: c_int) {
    // 从 multi handle 中移除
    unsafe {
      (self.lib.multi_remove_handle)(self.handle, easy_handle);
    }
    
    // 查找对应的回调 - 需要通过 easy_handle 找到对应的 curl id
    let handle_ptr = format!("0x{:x}", easy_handle as usize);
    
    if let Ok(mut callbacks) = self.curl_callbacks.lock() {
      // 遍历所有回调，找到匹配的 handle
      let mut found_key = None;
      for (key, callback) in callbacks.iter() {
        let curl_handle = callback.curl.get_handle();
        if curl_handle == easy_handle {
          found_key = Some(key.clone());
          break;
        }
      }
      
      if let Some(key) = found_key {
        if let Some(callback_info) = callbacks.remove(&key) {
          // 获取响应数据
          let headers = callback_info.curl.get_headers();
          let body = callback_info.curl.get_body();
          let response_code = callback_info.curl
            .get_info_number(crate::constants::CurlInfo::ResponseCode)
            .unwrap_or(-1);

          let curl_result = CurlResult {
            result_code,
            response_code,
            headers,
            body,
          };

          // 调用回调
          if result_code == 0 {
            if let Some(success_callback) = callback_info.success_callback {
              println!("Calling success callback");
              let _ = success_callback.call(
                Ok(curl_result),
                napi::threadsafe_function::ThreadsafeFunctionCallMode::NonBlocking,
              );
            }
          } else {
            if let Some(error_callback) = callback_info.error_callback {
              println!("Calling error callback");
              let error_msg = format!("Curl error: {}", result_code);
              let _ = error_callback.call(
                Ok(error_msg),
                napi::threadsafe_function::ThreadsafeFunctionCallMode::NonBlocking,
              );
            }
          }
        }
      }
    }
  }

  /// 处理 socket 事件
  fn handle_socket_action(&self, sockfd: c_int, what: c_int) {
    println!("handle_socket_action: sockfd={}, what={}", sockfd, what);
    self.perform_socket_action(sockfd, what);
  }

  /// 处理定时器事件
  fn handle_timer_action_simple(&self, timeout_ms: c_long) {
    println!("handle_timer_action_simple: timeout_ms={}", timeout_ms);
    
    if timeout_ms >= 0 {
      if timeout_ms == 0 {
        // 立即处理
        self.perform_socket_action(CURL_SOCKET_TIMEOUT, 0);
      } else {
        println!("Setting timer for {} ms", timeout_ms);
        // TODO: 实现定时器逻辑
      }
    }
  }

  /// 手动检查完成状态
  #[napi]
  pub fn check_for_completion(&self) {
    println!("Manual check for completion");
    self.perform_socket_action(CURL_SOCKET_TIMEOUT, 0);
  }

  /// 获取运行中的请求数量
  #[napi]
  pub fn get_active_count(&self) -> i32 {
    unsafe { *self.running_count.get() }
  }

  #[napi]
  pub fn error(&self, err: i64) -> String {
    unsafe {
      let url_ptr = (self.lib.multi_strerror)(err as c_int);
      let cstr = std::ffi::CStr::from_ptr(url_ptr);
      cstr.to_string_lossy().to_string()
    }
  }
}

impl Drop for CurlMulti {
  fn drop(&mut self) {
    println!("CurlMulti::drop called");
    if !self.handle.is_null() {
      unsafe {
        (self.lib.multi_cleanup)(self.handle);
      }
    }
  }
}