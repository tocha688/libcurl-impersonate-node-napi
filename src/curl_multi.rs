use std::{
  ffi::{c_int, c_long, c_void},
  sync::{Arc, Mutex},
};

use napi::{bindgen_prelude::*, threadsafe_function::ThreadsafeFunction};
use napi_derive::napi;

use crate::{
  constants::CurlMOpt,
  curl::Curl,
  loader::{napi_load_library, CurlFunctions, CurlHandle, CurlMultiHandle},
  utils::get_ptr_address,
};
use crate::{loader::CurlWaitFd, log_info};

#[napi(object)]
pub struct CurlMsgResult {
  pub msg: i64,
  pub easy_handle: i64,
  pub easy_id: String,
  pub data: CurlMsgDataResult,
}

#[napi(object)]
pub struct CurlMsgDataResult {
  pub whatever: i64,
  pub result: i32,
}

#[napi(object)]
pub struct SocketData {
  pub curl_id: String,
  pub sockfd: i32,
  pub what: i32,
}

#[napi(object)]
pub struct TimerData {
  pub multi_id: String,
  pub timeout_ms: i64,
}

#[derive(Debug)]
struct RawMulti {
  handle: CurlMultiHandle,
  lib: &'static CurlFunctions,
}

struct MultiData {
  socket: Box<dyn FnMut(SocketData) + Send>,
  timer: Box<dyn FnMut(TimerData) -> bool + Send>,
}

#[napi(js_name = "CurlMulti")]
pub struct CurlMulti {
  pub closed: bool,
  raw: Arc<RawMulti>,
  data: Arc<Mutex<MultiData>>,
  socket_data_ptr: Option<*const Mutex<MultiData>>,
  timer_data_ptr: Option<*const Mutex<MultiData>>,
}

// 手动实现 Send 和 Sync trait
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

    let multi = Self {
      raw: Arc::new(RawMulti { handle, lib }),
      data: Arc::new(Mutex::new(MultiData {
        socket: Box::new(|_| {}),
        timer: Box::new(|_| true),
      })),
      closed: false,
      socket_data_ptr: None,
      timer_data_ptr: None,
    };

    // multi.setup_default_callbacks()?;
    Ok(multi)
  }

  fn result(&self, code: i32) -> Result<()> {
    if code != 0 {
      Err(Error::new(
        Status::GenericFailure,
        format!(
          "failed with code: {} message:{}",
          code,
          self.error(code.into())
        ),
      ))
    } else {
      Ok(())
    }
  }

  pub fn set_opt(&self, option: CurlMOpt, value: *const c_void) -> Result<()> {
    self.check_close()?;
    log_info!(
      "Curl",
      "Setting option: {:?} with value: {:?}",
      option,
      value
    );
    self.result(unsafe { (self.raw.lib.multi_setopt)(self.raw.handle, option as c_int, value) })
  }
  /// 设置字符串选项
  #[napi]
  pub fn set_opt_string(&self, option: CurlMOpt, value: String) -> Result<()> {
    let c_str = std::ffi::CString::new(value).unwrap();
    self.set_opt(option, c_str.as_ptr() as *const c_void)
  }

  /// 设置长整型选项
  #[napi]
  pub fn set_opt_long(&self, option: CurlMOpt, value: i64) -> Result<()> {
    self.set_opt(option, value as *const c_void)
  }

  /// 设置boolean
  #[napi]
  pub fn set_opt_bool(&self, option: CurlMOpt, value: bool) -> Result<()> {
    self.set_opt(option, if value { 1 } else { 0 } as *const c_void)
  }

  /// 传入bytes
  #[napi]
  pub fn set_opt_bytes(&self, option: CurlMOpt, body: Vec<u8>) -> Result<()> {
    self.set_opt(option, body.as_ptr() as *const c_void)
  }

  #[napi]
  pub fn set_opt_buffer(&self, option: CurlMOpt, body: Buffer) -> Result<()> {
    self.set_opt(option, body.as_ptr() as *const c_void)
  }

  pub fn check_close(&self) -> Result<()> {
    if self.closed {
      return Err(Error::from_reason("CurlMulti has been closed"));
    }
    // Check if the handle is valid
    if self.raw.handle.is_null() {
      return Err(Error::from_reason("Curl multi handle is null"));
    }
    Ok(())
  }

  #[napi(ts_args_type = "callback: (result: {curl_id:string,sockfd:number,what:number}) => void")]
  pub fn set_socket_callback(&mut self, callback: ThreadsafeFunction<SocketData>) -> Result<()> {
    self.check_close()?;
    let tsfn = Arc::new(callback);
    log_info!("CurlMulti", "Setting socket callback");
    if let Ok(mut data) = self.data.lock() {
      data.socket = Box::new(move |sdata| {
        log_info!(
          "CurlMulti",
          "Socket callback: curl_id={}, sockfd={}, what={}",
          sdata.curl_id,
          sdata.sockfd,
          sdata.what
        );
        let _ = tsfn.call(
          Ok(sdata),
          napi::threadsafe_function::ThreadsafeFunctionCallMode::NonBlocking,
        );
      });
    }

    // 如果之前有设置过回调，先清理旧的指针
    if let Some(old_ptr) = self.socket_data_ptr {
      unsafe { Arc::from_raw(old_ptr) };
    }

    // 将 Arc 转换为原始指针并传递给 libcurl
    let data_ptr = Arc::into_raw(Arc::clone(&self.data));
    self.socket_data_ptr = Some(data_ptr);

    unsafe {
      (self.raw.lib.multi_setopt)(
        self.raw.handle,
        CurlMOpt::SocketFunction as c_int,
        socket_callback as *const c_void,
      );
      (self.raw.lib.multi_setopt)(
        self.raw.handle,
        CurlMOpt::SocketData as c_int,
        data_ptr as *const c_void,
      );
    }
    Ok(())
  }

  #[napi(ts_args_type = "callback: (result: {multi_id:string,timeout_ms:number}) => void")]
  pub fn set_timer_callback(&mut self, callback: ThreadsafeFunction<TimerData>) -> Result<()> {
    self.check_close()?;
    let tsfn = Arc::new(callback);
    log_info!("CurlMulti", "Setting timer callback");
    if let Ok(mut data) = self.data.lock() {
      data.timer = Box::new(move |tdata| {
        log_info!(
          "CurlMulti",
          "Timer callback: multi_id={}, timeout_ms={}",
          tdata.multi_id,
          tdata.timeout_ms
        );
        let _ = tsfn.call(
          Ok(tdata),
          napi::threadsafe_function::ThreadsafeFunctionCallMode::NonBlocking,
        );
        true
      });
    }

    // 如果之前有设置过回调，先清理旧的指针
    if let Some(old_ptr) = self.timer_data_ptr {
      unsafe { Arc::from_raw(old_ptr) };
    }

    // 将 Arc 转换为原始指针并传递给 libcurl
    let data_ptr = Arc::into_raw(Arc::clone(&self.data));
    self.timer_data_ptr = Some(data_ptr);

    unsafe {
      (self.raw.lib.multi_setopt)(
        self.raw.handle,
        CurlMOpt::TimerFunction as c_int,
        timer_callback as *const c_void,
      );
      (self.raw.lib.multi_setopt)(
        self.raw.handle,
        CurlMOpt::TimerData as c_int,
        data_ptr as *const c_void,
      );
    }
    Ok(())
  }

  #[napi]
  pub fn add_handle(&self, curl: &Curl) -> Result<i32> {
    self.check_close()?;
    let handle = curl.get_handle();
    if handle.is_null() {
      return Err(Error::from_reason("Invalid curl handle"));
    }
    curl.init();
    log_info!("CurlMulti", "Adding handle: {}", get_ptr_address(handle));
    unsafe { Ok((self.raw.lib.multi_add_handle)(self.raw.handle, handle)) }
  }

  #[napi]
  pub fn remove_handle(&self, curl: &Curl) -> Result<i32> {
    self.check_close()?;
    let handle = curl.get_handle();
    if handle.is_null() {
      return Err(Error::from_reason("Invalid curl handle"));
    }
    log_info!("CurlMulti", "Removing handle: {}", get_ptr_address(handle));
    unsafe { Ok((self.raw.lib.multi_remove_handle)(self.raw.handle, handle)) }
  }

  #[napi]
  pub fn error(&self, err: i64) -> String {
    log_info!("CurlMulti", "Getting error for code: {}", err);
    unsafe {
      let url_ptr = (self.raw.lib.multi_strerror)(err as c_int);
      let cstr = std::ffi::CStr::from_ptr(url_ptr);
      cstr.to_string_lossy().to_string()
    }
  }

  #[napi]
  pub fn perform(&self) -> Result<i32> {
    self.check_close()?;
    let mut remaining = 0;
    log_info!("CurlMulti", "Performing multi operation");
    unsafe {
      let result = (self.raw.lib.multi_perform)(self.raw.handle, &mut remaining);
      if result != 0 && result != 1 {
        return Err(Error::from_reason(format!("Perform failed: {}", result)));
      }
    }
    Ok(remaining)
  }

  #[napi]
  pub async fn poll(&self, timeout_ms: i32) -> Result<i32> {
    self.check_close()?;
    // 只 move 原始指针
    let handle = self.raw.handle as usize;
    spawn_blocking(move || {
      let mut remaining = 0;
      unsafe {
        let extra_fds: *mut CurlWaitFd = std::ptr::null_mut();
        // 恢复 lib 的引用
        let lib = napi_load_library()?;
        let code = (lib.multi_poll)(
          handle as CurlMultiHandle,
          extra_fds,
          0,
          timeout_ms,
          &mut remaining,
        );
        if code != 0 {
          return Err(Error::from_reason(format!("failed with code: {}", code)));
        }
      }
      Ok(remaining)
    })
    .await
    .map_err(|e| Error::from_reason(format!("Tokio join error: {e}")))?
  }

  #[napi]
  pub async fn wait(&self, timeout_ms: i32) -> Result<i32> {
    self.check_close()?;
    // 只 move 原始指针
    let handle = self.raw.handle as usize;
    spawn_blocking(move || {
      let mut remaining = 0;
      unsafe {
        let extra_fds: *mut CurlWaitFd = std::ptr::null_mut();
        // 恢复 lib 的引用
        let lib = napi_load_library()?;
        let code = (lib.multi_wait)(
          handle as CurlMultiHandle,
          extra_fds,
          0,
          timeout_ms,
          &mut remaining,
        );
        if code != 0 {
          return Err(Error::from_reason(format!("failed with code: {}", code)));
        }
      }
      Ok(remaining)
    })
    .await
    .map_err(|e| Error::from_reason(format!("Tokio join error: {e}")))?
  }

  #[napi]
  pub fn get_running_handles(&self) -> Result<i32> {
    self.check_close()?;
    let mut remaining = 0;
    log_info!("CurlMulti", "Getting running handles");
    unsafe {
      let result = (self.raw.lib.multi_perform)(self.raw.handle, &mut remaining);
      if result != 0 && result != 1 {
        return Err(Error::from_reason(format!(
          "Get running handles failed: {}",
          result
        )));
      }
    }
    Ok(remaining)
  }

  #[napi]
  pub fn socket_action(&self, socket: i64, what: i64) -> Result<i32> {
    self.check_close()?;
    let mut remaining = 0;
    log_info!(
      "CurlMulti",
      "Performing socket action: socket={}, what={}",
      socket,
      what
    );
    unsafe {
      let result = (self.raw.lib.multi_socket_action)(
        self.raw.handle,
        socket as c_int,
        what as c_int,
        &mut remaining,
      );
      if result != 0 {
        return Err(Error::from_reason(format!("Action failed: {}", result)));
      }
    }
    Ok(remaining)
  }

  #[napi]
  pub fn info_read(&self) -> Result<Option<CurlMsgResult>> {
    self.check_close()?;
    if self.raw.handle.is_null() {
      return Err(Error::from_reason("Curl multi handle is null"));
    }
    log_info!("CurlMulti", "Reading info from multi handle");

    let mut msgs_left = 0;
    let msg_ptr = unsafe { (self.raw.lib.multi_info_read)(self.raw.handle, &mut msgs_left) };

    // println!("info_read: msg_ptr={:p}, msgs_left={}", msg_ptr, msgs_left);

    if msg_ptr.is_null() {
      // 获取当前运行的传输数量来调试
      let mut running = 0;
      unsafe {
        let result = (self.raw.lib.multi_perform)(self.raw.handle, &mut running);
        if result != 0 {
          return Err(Error::from_reason(format!("Info read  failed: {}", result)));
        }
      }

      // 也检查一下 multi handle 的状态
      // println!("info_read: multi_handle={:p}", self.raw.handle);

      return Ok(None);
    }

    // 正确解引用指针来访问结构体字段
    let curl_msg = unsafe { &*msg_ptr };

    // println!(
    //   "info_read: found message - msg={}, easy_handle={:p}, result={}",
    //   curl_msg.msg,
    //   curl_msg.easy_handle,
    //   unsafe { curl_msg.data.result }
    // );

    Ok(Some(CurlMsgResult {
      msg: curl_msg.msg as i64,
      easy_handle: curl_msg.easy_handle as i64,
      easy_id: get_ptr_address(curl_msg.easy_handle),
      data: CurlMsgDataResult {
        whatever: unsafe { curl_msg.data.whatever as i64 },
        result: unsafe { curl_msg.data.result as i32 },
      },
    }))
  }

  #[napi]
  pub fn close(&mut self) {
    if self.closed || self.raw.handle.is_null() {
      return;
    }
    log_info!(
      "CurlMulti",
      "Closing CurlMulti handle: {}",
      get_ptr_address(self.raw.handle)
    );
    self.closed = true;

    unsafe {
      (self.raw.lib.multi_setopt)(
        self.raw.handle,
        CurlMOpt::SocketFunction as c_int,
        std::ptr::null() as *const c_void,
      );
      (self.raw.lib.multi_setopt)(
        self.raw.handle,
        CurlMOpt::SocketData as c_int,
        std::ptr::null() as *const c_void,
      );

      (self.raw.lib.multi_setopt)(
        self.raw.handle,
        CurlMOpt::TimerFunction as c_int,
        std::ptr::null() as *const c_void,
      );
      (self.raw.lib.multi_setopt)(
        self.raw.handle,
        CurlMOpt::TimerData as c_int,
        std::ptr::null() as *const c_void,
      );

      (self.raw.lib.multi_cleanup)(self.raw.handle);
    }

    // 清理分配的指针
    if let Some(ptr) = self.socket_data_ptr.take() {
      unsafe { Arc::from_raw(ptr) };
    }
    if let Some(ptr) = self.timer_data_ptr.take() {
      unsafe { Arc::from_raw(ptr) };
    }

    if let Ok(mut data) = self.data.lock() {
      data.socket = Box::new(|_| {});
      data.timer = Box::new(|_| true);
    }
  }
}

impl Drop for CurlMulti {
  fn drop(&mut self) {
    self.close();
  }
}

extern "C" fn socket_callback(
  _easy: CurlHandle,
  sockfd: c_int,
  what: c_int,
  userptr: *mut c_void,
  _socketp: *mut c_void,
) -> c_int {
  // println!(
  //   "socket_callback called: easy={:p}, sockfd={}, what={}",
  //   _easy, sockfd, what
  // );
  if userptr.is_null() {
    return 0;
  }

  // 从原始指针重新构造 Arc，但不释放所有权
  let data_arc = unsafe { Arc::from_raw(userptr as *const Mutex<MultiData>) };
  let result = if let Ok(mut data) = data_arc.try_lock() {
    (data.socket)(SocketData {
      curl_id: get_ptr_address(_easy),
      sockfd,
      what,
    });
    0
  } else {
    0
  };

  // 重新泄漏 Arc 避免释放内存
  std::mem::forget(data_arc);
  result
}

extern "C" fn timer_callback(
  _multi: CurlMultiHandle,
  timeout_ms: c_long,
  userptr: *mut c_void,
) -> c_int {
  // println!(
  //   "timer_callback called: multi={:p}, timeout_ms={}",
  //   _multi, timeout_ms
  // );
  if userptr.is_null() {
    return 0;
  }

  // 从原始指针重新构造 Arc，但不释放所有权
  let data_arc = unsafe { Arc::from_raw(userptr as *const Mutex<MultiData>) };
  let result = if let Ok(mut data) = data_arc.try_lock() {
    let keep_going = (data.timer)(TimerData {
      multi_id: get_ptr_address(_multi),
      timeout_ms: timeout_ms as i64,
    });
    if keep_going {
      0
    } else {
      -1
    }
  } else {
    0
  };

  // 重新泄漏 Arc 避免释放内存
  std::mem::forget(data_arc);
  result
}
