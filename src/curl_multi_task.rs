use napi::{bindgen_prelude::*, Env, JsString, Result, Task};
use std::sync::Arc;

use crate::curl::Curl;

pub struct CurlMultiTask {
  curl: Arc<Curl>,
}

impl CurlMultiTask {
  pub fn new(curl: Arc<Curl>) -> Self {
    Self { curl }
  }
}

unsafe impl Send for CurlMultiTask {}
unsafe impl Sync for CurlMultiTask {}

impl Task for CurlMultiTask {
  type Output = i32;
  type JsValue = napi::JsNumber;

  fn compute(&mut self) -> Result<Self::Output> {
    // 确保 curl 已初始化
    self.curl.init();
    
    // 执行 curl 请求
    let result = self.curl.perform();
    
    Ok(result)
  }

  fn resolve(&mut self, env: Env, output: i32) -> Result<Self::JsValue> {
    env.create_int32(output)
  }

  fn reject(&mut self, _env: napi::Env, err: napi::Error) -> napi::Result<Self::JsValue> {
    Err(err)
  }
}