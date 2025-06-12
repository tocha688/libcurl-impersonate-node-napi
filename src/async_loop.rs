use napi::threadsafe_function::{
  ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi::{JsFunction, Result};
use napi_derive::napi;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::Interest;
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

// 全局事件循环管理器
static GLOBAL_MANAGER: Lazy<Arc<Mutex<EventLoopManager>>> =
  Lazy::new(|| Arc::new(Mutex::new(EventLoopManager::new())));

#[derive(Clone)]
struct SocketHandler {
  read_callbacks: Vec<ThreadsafeFunction<SocketEvent, ErrorStrategy::Fatal>>,
  write_callbacks: Vec<ThreadsafeFunction<SocketEvent, ErrorStrategy::Fatal>>,
  monitor_handle: Option<String>, // 监听任务的唯一标识
}

struct EventLoopManager {
  socket_handlers: HashMap<i32, SocketHandler>,
  monitor_tasks: HashMap<String, tokio::task::JoinHandle<()>>,
}

#[derive(Clone)]
struct SocketEvent {
  sockfd: i32,
  event_type: u32,
}

impl EventLoopManager {
  fn new() -> Self {
    Self {
      socket_handlers: HashMap::new(),
      monitor_tasks: HashMap::new(),
    }
  }
}

#[napi]
pub struct AsyncEventLoop {}

#[napi]
impl AsyncEventLoop {
  #[napi(constructor)]
  pub fn new() -> Self {
    Self {}
  }

  #[napi(ts_args_type = "sockfd: number, callback: (sockfd: number, event_type: number) => void")]
  pub fn add_reader(&self, sockfd: i32, callback: JsFunction) -> Result<()> {
    let tsfn: ThreadsafeFunction<SocketEvent, ErrorStrategy::Fatal> = callback
      .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<SocketEvent>| {
        let event = ctx.value;
        Ok(vec![
          ctx.env.create_int32(event.sockfd)?,
          ctx.env.create_uint32(event.event_type)?,
        ])
      })?;

    let manager = Arc::clone(&GLOBAL_MANAGER);

    {
      let mut manager_lock = manager.lock().unwrap();

      // 获取或创建 socket 处理器
      let handler = manager_lock
        .socket_handlers
        .entry(sockfd)
        .or_insert_with(|| SocketHandler {
          read_callbacks: Vec::new(),
          write_callbacks: Vec::new(),
          monitor_handle: None,
        });

      // 添加读回调
      handler.read_callbacks.push(tsfn);

      // 如果还没有监听任务，启动一个
      if handler.monitor_handle.is_none() {
        let monitor_id = Uuid::new_v4().to_string();
        handler.monitor_handle = Some(monitor_id.clone());

        let task = Self::start_socket_monitoring(sockfd, Arc::clone(&manager));
        manager_lock.monitor_tasks.insert(monitor_id, task);
      }
    }

    Ok(())
  }

  #[napi(ts_args_type = "sockfd: number, callback: (sockfd: number, event_type: number) => void")]
  pub fn add_writer(&self, sockfd: i32, callback: JsFunction) -> Result<()> {
    let tsfn: ThreadsafeFunction<SocketEvent, ErrorStrategy::Fatal> = callback
      .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<SocketEvent>| {
        let event = ctx.value;
        Ok(vec![
          ctx.env.create_int32(event.sockfd)?,
          ctx.env.create_uint32(event.event_type)?,
        ])
      })?;

    let manager = Arc::clone(&GLOBAL_MANAGER);

    {
      let mut manager_lock = manager.lock().unwrap();

      let handler = manager_lock
        .socket_handlers
        .entry(sockfd)
        .or_insert_with(|| SocketHandler {
          read_callbacks: Vec::new(),
          write_callbacks: Vec::new(),
          monitor_handle: None,
        });

      // 添加写回调
      handler.write_callbacks.push(tsfn);

      // 如果还没有监听任务，启动一个
      if handler.monitor_handle.is_none() {
        let monitor_id = Uuid::new_v4().to_string();
        handler.monitor_handle = Some(monitor_id.clone());

        let task = Self::start_socket_monitoring(sockfd, Arc::clone(&manager));
        manager_lock.monitor_tasks.insert(monitor_id, task);
      }
    }

    Ok(())
  }

  // 私有方法：清理空的 socket handler
  fn cleanup_socket_if_empty(
    manager_lock: &mut std::sync::MutexGuard<EventLoopManager>,
    sockfd: i32,
  ) {
    let should_cleanup = manager_lock
      .socket_handlers
      .get(&sockfd)
      .map(|h| h.read_callbacks.is_empty() && h.write_callbacks.is_empty())
      .unwrap_or(false);

    if should_cleanup {
      let monitor_id = manager_lock
        .socket_handlers
        .get(&sockfd)
        .and_then(|h| h.monitor_handle.clone());

      if let Some(monitor_id) = monitor_id {
        if let Some(task) = manager_lock.monitor_tasks.remove(&monitor_id) {
          task.abort();
        }
      }

      manager_lock.socket_handlers.remove(&sockfd);
    }
  }

  #[napi]
  pub fn remove_reader(&self, sockfd: i32) -> Result<()> {
    let manager = Arc::clone(&GLOBAL_MANAGER);
    let mut manager_lock = manager.lock().unwrap();

    if let Some(handler) = manager_lock.socket_handlers.get_mut(&sockfd) {
      handler.read_callbacks.clear();
    }

    Self::cleanup_socket_if_empty(&mut manager_lock, sockfd);
    Ok(())
  }

  #[napi]
  pub fn remove_writer(&self, sockfd: i32) -> Result<()> {
    let manager = Arc::clone(&GLOBAL_MANAGER);
    let mut manager_lock = manager.lock().unwrap();

    if let Some(handler) = manager_lock.socket_handlers.get_mut(&sockfd) {
      handler.write_callbacks.clear();
    }

    Self::cleanup_socket_if_empty(&mut manager_lock, sockfd);
    Ok(())
  }

  #[napi(ts_args_type = "callback: (result: number) => void")]
  pub fn call_later(&self, delay_ms: i64, callback: JsFunction) -> Result<String> {
    let tsfn: ThreadsafeFunction<i64, ErrorStrategy::Fatal> =
      callback.create_threadsafe_function(0, |ctx| Ok(vec![ctx.env.create_int64(ctx.value)?]))?;

    let delay = Duration::from_millis(delay_ms as u64);
    let timer_id = Uuid::new_v4().to_string();

    tokio::spawn(async move {
      sleep(delay).await;
      let _ = tsfn.call(delay_ms, ThreadsafeFunctionCallMode::NonBlocking);
    });

    Ok(timer_id)
  }

  // 启动 socket 监听任务
  fn start_socket_monitoring(
    sockfd: i32,
    manager: Arc<Mutex<EventLoopManager>>,
  ) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
      // 尝试从文件描述符创建 TcpStream
      let stream_result = unsafe { Self::create_tcp_stream_from_fd(sockfd) };

      match stream_result {
        Ok(stream) => {
          loop {
            // 检查是否还有活跃的监听器
            let (has_readers, has_writers) = {
              let manager_lock = manager.lock().unwrap();
              if let Some(handler) = manager_lock.socket_handlers.get(&sockfd) {
                (
                  !handler.read_callbacks.is_empty(),
                  !handler.write_callbacks.is_empty(),
                )
              } else {
                (false, false)
              }
            };

            if !has_readers && !has_writers {
              break;
            }
            // 根据需要监听的事件类型决定监听策略
            tokio::select! {
                // 监听读事件
                read_result = stream.ready(Interest::READABLE), if has_readers => {
                    if read_result.is_ok() {
                        Self::trigger_read_callbacks(sockfd, &manager).await;
                    }
                }
                // 监听写事件
                write_result = stream.ready(Interest::WRITABLE), if has_writers => {
                    if write_result.is_ok() {
                        Self::trigger_write_callbacks(sockfd, &manager).await;
                    }
                }
                // 超时检查（避免无限等待）
                _ = sleep(Duration::from_millis(50)) => {
                    // 定期检查是否还需要监听
                    continue;
                }
            }

            // 短暂暂停，避免过度占用 CPU
            sleep(Duration::from_millis(1)).await;
          }
        }
        Err(_) => {
          // println!("Failed to create stream from fd {}: {:?}", sockfd, e);
        }
      }

      // 任务结束时清理
      let mut manager_lock = manager.lock().unwrap();
      manager_lock.socket_handlers.remove(&sockfd);
    })
  }

  async fn trigger_read_callbacks(sockfd: i32, manager: &Arc<Mutex<EventLoopManager>>) {
    let callbacks = {
      let manager_lock = manager.lock().unwrap();
      if let Some(handler) = manager_lock.socket_handlers.get(&sockfd) {
        handler.read_callbacks.clone()
      } else {
        Vec::new()
      }
    };

    for callback in callbacks {
      let event = SocketEvent {
        sockfd,
        event_type: 0x01, // CURL_CSELECT_IN
      };
      let _ = callback.call(event, ThreadsafeFunctionCallMode::NonBlocking);
    }
  }

  async fn trigger_write_callbacks(sockfd: i32, manager: &Arc<Mutex<EventLoopManager>>) {
    let callbacks = {
      let manager_lock = manager.lock().unwrap();
      if let Some(handler) = manager_lock.socket_handlers.get(&sockfd) {
        handler.write_callbacks.clone()
      } else {
        Vec::new()
      }
    };

    for callback in callbacks {
      let event = SocketEvent {
        sockfd,
        event_type: 0x02, // CURL_CSELECT_OUT
      };
      let _ = callback.call(event, ThreadsafeFunctionCallMode::NonBlocking);
    }
  }

  // 平台特定的文件描述符转换
  #[cfg(unix)]
  unsafe fn create_tcp_stream_from_fd(fd: i32) -> std::io::Result<TcpStream> {
    use std::os::unix::io::FromRawFd;
    // 先创建 std::net::TcpStream，然后转换为 tokio::net::TcpStream
    let std_stream = std::net::TcpStream::from_raw_fd(fd);
    std_stream.set_nonblocking(true)?; // 确保是非阻塞的
    TcpStream::from_std(std_stream)
  }

  #[cfg(windows)]
  unsafe fn create_tcp_stream_from_fd(fd: i32) -> std::io::Result<TcpStream> {
    use std::os::windows::io::FromRawSocket;
    // 先创建 std::net::TcpStream，然后转换为 tokio::net::TcpStream
    let std_stream = std::net::TcpStream::from_raw_socket(fd.try_into().unwrap());
    std_stream.set_nonblocking(true)?; // 确保是非阻塞的
    TcpStream::from_std(std_stream)
  }
}
