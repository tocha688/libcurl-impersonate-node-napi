use std::{
  ffi::{c_int, c_long, c_void},
  sync::{Arc, Mutex},
};

use napi::{
  bindgen_prelude::*, threadsafe_function::ThreadSafeCallContext,
};
use napi_derive::{ napi};

use crate::{
  constants::CurlMOpt,
  curl::Curl,
  loader::{napi_load_library, CurlFunctions, CurlHandle, CurlMsg, CurlMultiHandle},
  utils::get_ptr_address,
};

#[napi(object)]
pub struct CurlMsgResult {
  pub msg: i64,
  pub easy_handle: i32,
  pub data: CurlMsgDataResult,
}

#[napi(object)]
pub struct CurlMsgDataResult {
  pub whatever: i64,
  pub result: i32,
}

pub struct SocketData {
  pub curl_id: String,
  pub socket: i32,
  pub what: i32,
}

pub struct TimerData {
  pub multi_id: String,
  pub timeout_ms: i32,
}

#[derive(Debug)]
struct RawMulti {
  handle: CurlMultiHandle,
  lib: &'static CurlFunctions,
}

// Multi 数据结构，存储回调函数
struct MultiData {
  socket: Box<dyn FnMut(SocketData) + Send>,
  timer: Box<dyn FnMut(TimerData) -> bool + Send>,
}

// Multi 主结构
#[napi(js_name = "CurlMulti2")]
pub struct CurlMulti {
  raw: Arc<RawMulti>,
  data: Arc<Mutex<MultiData>>,
}

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
    };

    // 设置默认的 socket 和 timer 回调函数
    multi.setup_default_callbacks()?;

    Ok(multi)
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

      // let ptr = Arc::into_raw(self.data.clone()) as *const c_void;
      let ptr = Arc::into_raw(self.data.clone()) as *const c_void;
      let result = (self.raw.lib.multi_setopt)(self.raw.handle, CurlMOpt::SocketData as c_int, ptr);
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

      let result = (self.raw.lib.multi_setopt)(self.raw.handle, CurlMOpt::TimerData as c_int, ptr);
      if result != 0 {
        return Err(Error::from_reason("Failed to set timer data"));
      }
    }

    Ok(())
  }

  /// 设置 Socket 回调函数
  #[napi(ts_args_type = "callback: (err: null | Error, result: {curl_id:string,socket:number,what:number}) => void")]
  pub fn set_socket_callback(&self, env: Env, callback: JsFunction) -> Result<()> {
    // 创建线程安全函数
    let tsfn =
      env.create_threadsafe_function(&callback, 0, |ctx: ThreadSafeCallContext<SocketData>| {
        let sdata = ctx.value;
        let mut obj = ctx.env.create_object()?;
        obj.set("curl_id", sdata.curl_id)?;
        obj.set("socket", sdata.socket)?;
        obj.set("what", sdata.what)?;
        Ok(vec![obj])
      })?;

    // 存储回调函数
    if let Ok(mut data) = self.data.lock() {
      // 更新内部回调以调用 JavaScript 函数
      data.socket = Box::new(move |sdata| {
        println!(
          "Socket callback: socket={}, events={:?}",
          sdata.socket, sdata.what
        );

        // 调用 JavaScript 回调
        let _ = tsfn.call(
          Ok(sdata),
          napi::threadsafe_function::ThreadsafeFunctionCallMode::NonBlocking,
        );
      });
    }

    Ok(())
  }

  /// 设置 Timer 回调函数
  #[napi(ts_args_type= "callback: (err: null | Error, result: {multi_id:string,timeout_ms:number}) => void")]
  pub fn set_timer_callback(&self, env: Env, callback: JsFunction) -> Result<()> {
    // 创建线程安全函数
    let tsfn = env.create_threadsafe_function(
      &callback,
      0,
      |ctx: ThreadSafeCallContext<TimerData>| {
        let tdata = ctx.value;
        //创建返回值
        let mut obj = ctx.env.create_object()?;
        obj.set("multi_id", tdata.multi_id)?;
        obj.set("timeout_ms", tdata.timeout_ms)?;
        Ok(vec![obj])
      },
    )?;

    // 更新内部回调
    if let Ok(mut data) = self.data.lock() {
      data.timer = Box::new(move |tdata| {
        println!("Timer callback: timeout={}ms", tdata.timeout_ms);

        // 调用 JavaScript 回调
        let _ = tsfn.call(
          Ok(tdata),
          napi::threadsafe_function::ThreadsafeFunctionCallMode::NonBlocking,
        );

        true
      });
    }

    Ok(())
  }

  #[napi]
  pub fn add_handle(&self, curl: &Curl) -> Result<i32> {
    let handle = curl.get_handle();
    if handle.is_null() {
      return Err(Error::from_reason("Invalid curl handle"));
    }
    unsafe { Ok((self.raw.lib.multi_add_handle)(self.raw.handle, handle)) }
  }

  #[napi]
  pub fn remove_handle(&self, curl: &Curl) -> Result<i32> {
    let handle = curl.get_handle();
    if handle.is_null() {
      return Err(Error::from_reason("Invalid curl handle"));
    }
    unsafe { Ok((self.raw.lib.multi_remove_handle)(self.raw.handle, handle)) }
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

  #[napi]
  pub fn perform(&self) -> Result<u32> {
    let mut remaining = 0;
    unsafe {
      let result = (self.raw.lib.multi_perform)(self.raw.handle, &mut remaining);
      if result != 0 && result != 1 {
        return Err(Error::from_reason(format!("Perform failed: {}", result)));
      }
    }
    Ok(remaining as u32)
  }

  #[napi]
  pub fn socket_action(&mut self, socket: c_int, what: c_int) -> Result<u32> {
    let mut remaining = 0;
    unsafe {
      let result =
        (self.raw.lib.multi_socket_action)(self.raw.handle, socket, what, &mut remaining);
      if result != 0 {
        return Err(Error::from_reason(format!("Action failed: {}", result)));
      }
    }

    Ok(remaining as u32)
  }

  #[napi]
  pub fn info_read(&self) -> Result<CurlMsgResult> {
    if self.raw.handle.is_null() {
      return Err(Error::from_reason("Curl multi handle is null"));
    }
    let mut msgs_left = 0i32;
    let msg = unsafe { (self.raw.lib.multi_info_read)(self.raw.handle, &mut msgs_left) };

    if msg.is_null() {
      return Err(Error::from_reason("CurlMsg is null"));
    }

    let curl_msg = unsafe { &*(msg as *const CurlMsg) };
    Ok(CurlMsgResult {
      msg: curl_msg.msg as i64,
      easy_handle: curl_msg.easy_handle as i32,
      data: CurlMsgDataResult {
        whatever: unsafe { curl_msg.data.whatever as i64 },
        result: unsafe { curl_msg.data.result as i32 },
      },
    })
  }

  #[napi]
  pub fn close(&self) {
    // 释放资源
    unsafe {
      (self.raw.lib.multi_cleanup)(self.raw.handle);
    }
    // 清理回调函数
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

// 回调函数实现 - 修正函数调用语法
extern "C" fn socket_callback(
  _easy: CurlHandle,
  socket: c_int,
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
    (data.socket)(SocketData {
      curl_id: get_ptr_address(_easy),
      socket,
      what,
    });
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
    (data.timer)(TimerData {
      multi_id: get_ptr_address(_multi),
      timeout_ms,
    })
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
