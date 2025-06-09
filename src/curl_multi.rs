use crate::constants::CurlMOpt;
use crate::curl::Curl;
use crate::loader::{napi_load_library, CurlFunctions, CurlHandle, CurlMsg, CurlMultiHandle};
use crate::utils::get_ptr_address;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::HashMap;
use std::marker;
use std::os::raw::{c_int, c_long, c_void};
use std::sync::{Arc, Mutex};

// 请求结果结构体
#[derive(Debug, Clone)]
pub struct CurlResult {
  pub result_code: i32,
  pub response_code: i64,
}

// 内部原始 Multi 句柄
#[derive(Debug)]
struct RawMulti {
  handle: CurlMultiHandle,
  lib: &'static CurlFunctions,
}

// Multi 数据结构，存储回调函数
struct MultiData {
  socket: Box<dyn FnMut(Socket, SocketEvents, usize) + Send>,
  timer: Box<dyn FnMut(Option<std::time::Duration>) -> bool + Send>,
  curl_callbacks: HashMap<String, CurlCallback>,
}

// 存储回调的结构体
struct CurlCallback {
  success_callback: Option<napi::threadsafe_function::ThreadsafeFunction<CurlResult>>,
  error_callback: Option<napi::threadsafe_function::ThreadsafeFunction<String>>,
  curl: Arc<Curl>,
}

// Socket 和 SocketEvents 类型定义
pub type Socket = c_int;

#[napi]
pub struct SocketEvents {
  bits: c_int,
}

// Multi 主结构
#[napi(js_name = "CurlMulti")]
pub struct Multi {
  raw: Arc<RawMulti>,
  data: Arc<Mutex<MultiData>>,
}

// EasyHandle 包装器，确保正确的生命周期管理
pub struct EasyHandle {
  guard: DetachGuard,
  easy: Curl,
  _marker: marker::PhantomData<&'static Multi>,
}

// 分离守卫，确保在 drop 时正确移除句柄
struct DetachGuard {
  multi: Arc<RawMulti>,
  easy: CurlHandle,
}

// 常量定义
const CURLMSG_DONE: c_int = 1;
const CURL_SOCKET_TIMEOUT: c_int = -1;

// 实现 Send 和 Sync
unsafe impl Send for Multi {}
unsafe impl Sync for Multi {}

#[napi]
impl Multi {
  /// Creates a new multi session through which multiple HTTP transfers can be initiated.
  #[napi(constructor)]
  pub fn new() -> Result<Self> {
    let lib = napi_load_library()?;
    let handle = unsafe { (lib.multi_init)() };

    if handle.is_null() {
      return Err(Error::from_reason("Failed to initialize curl multi handle"));
    }

    let multi = Multi {
      raw: Arc::new(RawMulti { handle, lib }),
      data: Arc::new(Mutex::new(MultiData {
        socket: Box::new(|socket, events, _token| {
          println!("Default socket callback: socket={}, events={:?}", socket, events.bits);
        }),
        timer: Box::new(|timeout| {
          if let Some(duration) = timeout {
            println!("Default timer callback: timeout={}ms", duration.as_millis());
          } else {
            println!("Default timer callback: no timeout");
          }
          true
        }),
        curl_callbacks: HashMap::new(),
      })),
    };

    // 设置默认的 socket 和 timer 回调函数
    multi.setup_default_callbacks()?;

    Ok(multi)
  }

  /// Inform of reads/writes available data given an action
  #[napi]
  pub fn action(&mut self, socket: Socket, events: &SocketEvents) -> Result<u32> {
    let mut remaining = 0;
    unsafe {
      let result =
        (self.raw.lib.multi_socket_action)(self.raw.handle, socket, events.bits, &mut remaining);
      if result != 0 {
        return Err(Error::from_reason(format!("Action failed: {}", result)));
      }
    }

    self.check_finished_requests();

    Ok(remaining as u32)
  }

  /// Inform libcurl that a timeout has expired
  #[napi]
  pub fn timeout(&mut self) -> Result<u32> {
    let mut remaining = 0;
    unsafe {
      let result =
        (self.raw.lib.multi_socket_action)(self.raw.handle, CURL_SOCKET_TIMEOUT, 0, &mut remaining);
      if result != 0 {
        return Err(Error::from_reason(format!(
          "Timeout action failed: {}",
          result
        )));
      }
    }

    self.check_finished_requests();

    Ok(remaining as u32)
  }

  /// Reads/writes available data from each easy handle
  #[napi]
  pub fn perform(&mut self) -> Result<u32> {
    let mut remaining = 0;
    unsafe {
      let result = (self.raw.lib.multi_perform)(self.raw.handle, &mut remaining);
      if result != 0 && result != 1 {
        return Err(Error::from_reason(format!("Perform failed: {}", result)));
      }
    }

    self.check_finished_requests();

    Ok(remaining as u32)
  }

  /// 异步执行单个请求
  #[napi]
  pub fn send(
    &mut self,
    env: Env,
    curl: &Curl,
    success_callback: JsFunction,
    error_callback: Option<JsFunction>,
  ) -> Result<()> {
    let handle_addr = curl.get_handle();
    let handle_key = get_ptr_address(handle_addr);
    println!("perform_async called for curl: {}", handle_key);

    if handle_addr.is_null() {
      return Err(Error::from_reason("Curl handle is null"));
    }

    // 并不会重置,这只是设置header和response的buffer指针
    curl.init();

    let success_tsfn = env.create_threadsafe_function(&success_callback, 0, |ctx| {
      let result: CurlResult = ctx.value;
      let mut obj = ctx.env.create_object()?;
      obj.set_named_property("resultCode", ctx.env.create_int32(result.result_code)?)?;
      obj.set_named_property("responseCode", ctx.env.create_int64(result.response_code)?)?;
      Ok(vec![obj])
    })?;

    let error_tsfn = if let Some(error_cb) = error_callback {
      Some(env.create_threadsafe_function(&error_cb, 0, |ctx| {
        let error_msg: String = ctx.value;
        Ok(vec![ctx.env.create_string(&error_msg)?])
      })?)
    } else {
      None
    };

    if let Ok(mut data) = self.data.lock() {
      data.curl_callbacks.insert(
        handle_key.clone(),
        CurlCallback {
          success_callback: Some(success_tsfn),
          error_callback: error_tsfn,
          curl: Arc::new(curl.clone()),
        },
      );
    }

    unsafe {
      let result = (self.raw.lib.multi_add_handle)(self.raw.handle, handle_addr);
      if result != 0 {
        return Err(Error::from_reason(format!(
          "Failed to add handle: {}",
          result
        )));
      }
    }

    self.perform()?;
    
    println!("Handle added successfully for curl: {}", handle_key);

    Ok(())
  }

  /// Get a pointer to the raw underlying CURLM handle
  #[napi]
  pub fn raw(&self) -> i64 {
    self.raw.handle as i64
  }

  /// 获取错误信息
  #[napi]
  pub fn error(&self, err: i64) -> String {
    unsafe {
      let url_ptr = (self.raw.lib.multi_strerror)(err as c_int);
      let cstr = std::ffi::CStr::from_ptr(url_ptr);
      cstr.to_string_lossy().to_string()
    }
  }

  /// 检查完成的请求
  fn check_finished_requests(&self) {
    if self.raw.handle.is_null() {
      return;
    }

    let mut msgs_left = 0i32;

    loop {
      let msg = unsafe { (self.raw.lib.multi_info_read)(self.raw.handle, &mut msgs_left) };

      if msg.is_null() {
        break;
      }

      let curl_msg = unsafe { &*(msg as *const CurlMsg) };

      if curl_msg.msg == CURLMSG_DONE {
        let easy_handle = curl_msg.easy_handle;
        let result_code = unsafe { curl_msg.data.result };

        println!(
          "Request completed: handle={:p}, result={}",
          easy_handle, result_code
        );

        unsafe {
          (self.raw.lib.multi_remove_handle)(self.raw.handle, easy_handle);
        }

        self.handle_completed_request(easy_handle, result_code);
      }

      if msgs_left == 0 {
        break;
      }
    }
  }

  /// 处理完成的请求
  fn handle_completed_request(&self, easy_handle: CurlHandle, result_code: c_int) {
    let handle_key = get_ptr_address(easy_handle);
    println!(
      "Handling completed request: handle={}, result={}",
      handle_key, result_code
    );

    if let Ok(mut data) = self.data.lock() {
      println!("Looking for callback with key: {}", handle_key);
      println!("Available callback keys: {:?}", data.curl_callbacks.keys().collect::<Vec<_>>());

      if let Some(callback_info) = data.curl_callbacks.remove(&handle_key) {
        println!("Found and removed callback for key: {}", handle_key);
        
        let response_code = callback_info
          .curl
          .get_info_number(crate::constants::CurlInfo::ResponseCode)
          .unwrap_or(-1);

        println!("Response code: {}", response_code);

        let curl_result = CurlResult {
          result_code,
          response_code,
        };

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
      } else {
        println!("No callback found for handle: {}", handle_key);
      }
    }
  }

  /// 设置默认回调函数
  fn setup_default_callbacks(&self) -> Result<()> {
    unsafe {
      // 设置 socket 回调
      let result = (self.raw.lib.multi_setopt)(
        self.raw.handle,
        CurlMOpt::SocketFunction as c_int,
        socket_callback as *const c_void,
      );
      if result != 0 {
        return Err(Error::from_reason("Failed to set socket function"));
      }

      let ptr = Arc::into_raw(self.data.clone()) as *const c_void;
      let result = (self.raw.lib.multi_setopt)(
        self.raw.handle,
        CurlMOpt::SocketData as c_int,
        ptr,
      );
      if result != 0 {
        return Err(Error::from_reason("Failed to set socket data"));
      }

      // 设置 timer 回调
      let result = (self.raw.lib.multi_setopt)(
        self.raw.handle,
        CurlMOpt::TimerFunction as c_int,
        timer_callback as *const c_void,
      );
      if result != 0 {
        return Err(Error::from_reason("Failed to set timer function"));
      }

      let result = (self.raw.lib.multi_setopt)(
        self.raw.handle,
        CurlMOpt::TimerData as c_int,
        ptr,
      );
      if result != 0 {
        return Err(Error::from_reason("Failed to set timer data"));
      }
    }

    Ok(())
  }
}

// 非 napi 方法的实现
impl Multi {
  /// Set the callback informed about what to wait for
  pub fn socket_function<F>(&self, f: F) -> Result<()>
  where
    F: FnMut(Socket, SocketEvents, usize) + Send + 'static,
  {
    self._socket_function(Box::new(f))
  }

  fn _socket_function(&self, f: Box<dyn FnMut(Socket, SocketEvents, usize) + Send>) -> Result<()> {
    if let Ok(mut data) = self.data.lock() {
      data.socket = f;
    }
    Ok(())
  }

  /// Set callback to receive timeout values
  pub fn timer_function<F>(&self, f: F) -> Result<()>
  where
    F: FnMut(Option<std::time::Duration>) -> bool + Send + 'static,
  {
    self._timer_function(Box::new(f))
  }

  fn _timer_function(
    &self,
    f: Box<dyn FnMut(Option<std::time::Duration>) -> bool + Send>,
  ) -> Result<()> {
    if let Ok(mut data) = self.data.lock() {
      data.timer = f;
    }
    Ok(())
  }

  /// Add an easy handle to a multi session
  pub fn add(&self, curl: Curl) -> Result<EasyHandle> {
    unsafe {
      let result = (self.raw.lib.multi_add_handle)(self.raw.handle, curl.get_handle());
      if result != 0 {
        return Err(Error::from_reason(format!(
          "Failed to add handle: {}",
          result
        )));
      }
    }

    Ok(EasyHandle {
      guard: DetachGuard {
        multi: self.raw.clone(),
        easy: curl.get_handle(),
      },
      easy: curl,
      _marker: marker::PhantomData,
    })
  }

  /// Remove an easy handle from this multi session
  pub fn remove(&self, mut easy_handle: EasyHandle) -> Result<Curl> {
    easy_handle.guard.detach()?;
    Ok(easy_handle.easy)
  }
}

// 实现 SocketEvents
#[napi]
impl SocketEvents {
  #[napi(constructor)]
  pub fn new() -> Self {
    SocketEvents { bits: 0 }
  }

  #[napi]
  pub fn input(&mut self, val: bool) {
    self.flag(1, val);
  }

  #[napi]
  pub fn output(&mut self, val: bool) {
    self.flag(2, val);
  }

  #[napi]
  pub fn error(&mut self, val: bool) {
    self.flag(4, val);
  }

  fn flag(&mut self, flag: c_int, val: bool) -> &mut Self {
    if val {
      self.bits |= flag;
    } else {
      self.bits &= !flag;
    }
    self
  }
}

// 实现 DetachGuard
impl DetachGuard {
  fn detach(&mut self) -> Result<()> {
    if !self.easy.is_null() {
      unsafe {
        let result = (self.multi.lib.multi_remove_handle)(self.multi.handle, self.easy);
        if result != 0 {
          return Err(Error::from_reason(format!(
            "Failed to remove handle: {}",
            result
          )));
        }
      }
      self.easy = std::ptr::null_mut();
    }
    Ok(())
  }
}

impl Drop for DetachGuard {
  fn drop(&mut self) {
    let _ = self.detach();
  }
}

// 实现 RawMulti 的 Drop
impl Drop for RawMulti {
  fn drop(&mut self) {
    if !self.handle.is_null() {
      unsafe {
        let _ = (self.lib.multi_setopt)(
          self.handle,
          CurlMOpt::SocketFunction as c_int,
          std::ptr::null::<c_void>(),
        );
        let _ = (self.lib.multi_setopt)(
          self.handle,
          CurlMOpt::SocketData as c_int,
          std::ptr::null::<c_void>(),
        );
        let _ = (self.lib.multi_setopt)(
          self.handle,
          CurlMOpt::TimerFunction as c_int,
          std::ptr::null::<c_void>(),
        );
        let _ = (self.lib.multi_setopt)(
          self.handle,
          CurlMOpt::TimerData as c_int,
          std::ptr::null::<c_void>(),
        );

        (self.lib.multi_cleanup)(self.handle);
      }
    }
  }
}

// 回调函数实现 - 修正函数调用语法
extern "C" fn socket_callback(
  _easy: CurlHandle,
  socket: Socket,
  what: c_int,
  userptr: *mut c_void,
  _socketp: *mut c_void,
) -> c_int {
  println!(
    "socket_callback called with socket: {}, what: {}",
    socket, what
  );
  if userptr.is_null() {
    return 0;
  }

  let data_ptr = userptr as *const Mutex<MultiData>;
  let data_arc = unsafe { Arc::from_raw(data_ptr) };

  if let Ok(mut data) = data_arc.try_lock() {
    let events = SocketEvents { bits: what };
    (data.socket)(socket, events, 0);
  }

  // 重要：不要忘记这个，否则会内存泄漏
  std::mem::forget(data_arc);

  0
}

extern "C" fn timer_callback(
  _multi: CurlMultiHandle,
  timeout_ms: c_long,
  userptr: *mut c_void,
) -> c_int {
  if userptr.is_null() {
    return 0;
  }

  println!("timer_callback called with timeout_ms: {}", timeout_ms);

  let data_ptr = userptr as *const Mutex<MultiData>;
  let data_arc = unsafe { Arc::from_raw(data_ptr) };

  let keep_going = if let Ok(mut data) = data_arc.try_lock() {
    let timeout = if timeout_ms == -1 {
      None
    } else {
      Some(std::time::Duration::from_millis(timeout_ms as u64))
    };
    (data.timer)(timeout)
  } else {
    false
  };

  // 重要：不要忘记这个，否则会内存泄漏
  std::mem::forget(data_arc);

  if keep_going {
    0
  } else {
    -1
  }
}
