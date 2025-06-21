use std::sync::atomic::{AtomicBool, Ordering};

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// 全局启用/禁用日志的标志
static LOGGING_ENABLED: AtomicBool = AtomicBool::new(false);
/// 当前日志级别
static mut CURRENT_LOG_LEVEL: LogLevel = LogLevel::Info;

/// 启用日志输出
pub fn enable_logging(enable: bool) {
    LOGGING_ENABLED.store(enable, Ordering::SeqCst);
}

/// 设置日志级别
pub fn set_log_level(level: LogLevel) {
    unsafe {
        CURRENT_LOG_LEVEL = level;
    }
}

/// 获取当前日志级别
pub fn get_log_level() -> LogLevel {
    unsafe {
        CURRENT_LOG_LEVEL
    }
}

/// 判断日志是否启用
pub fn is_logging_enabled() -> bool {
    LOGGING_ENABLED.load(Ordering::SeqCst)
}

/// 判断指定级别的日志是否应该输出
pub fn should_log(level: LogLevel) -> bool {
    if !is_logging_enabled() {
        return false;
    }

    match (unsafe { CURRENT_LOG_LEVEL }, level) {
        (LogLevel::Debug, _) => true,
        (LogLevel::Info, LogLevel::Info | LogLevel::Warn | LogLevel::Error) => true,
        (LogLevel::Warn, LogLevel::Warn | LogLevel::Error) => true,
        (LogLevel::Error, LogLevel::Error) => true,
        _ => false,
    }
}

/// 记录调试日志
pub fn log_debug(module: &str, message: &str) {
    if should_log(LogLevel::Debug) {
        println!("[DEBUG][{}] {}", module, message);
    }
}

/// 记录信息日志
pub fn log_info(module: &str, message: &str) {
    if should_log(LogLevel::Info) {
        println!("[INFO][{}] {}", module, message);
    }
}

/// 记录警告日志
pub fn log_warn(module: &str, message: &str) {
    if should_log(LogLevel::Warn) {
        println!("[WARN][{}] {}", module, message);
    }
}

/// 记录错误日志
pub fn log_error(module: &str, message: &str) {
    if should_log(LogLevel::Error) {
        eprintln!("[ERROR][{}] {}", module, message);
    }
}

// 便捷宏，支持格式化输出
#[macro_export]
macro_rules! log_debug {
    ($module:expr, $($arg:tt)*) => {
        if $crate::logger::should_log($crate::logger::LogLevel::Debug) {
            $crate::logger::log_debug($module, &format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_info{
    ($module:expr, $($arg:tt)*) => {
        if $crate::logger::should_log($crate::logger::LogLevel::Info) {
            $crate::logger::log_info($module, &format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_warn{
    ($module:expr, $($arg:tt)*) => {
        if $crate::logger::should_log($crate::logger::LogLevel::Warn) {
            $crate::logger::log_warn($module, &format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_error{
    ($module:expr, $($arg:tt)*) => {
        if $crate::logger::should_log($crate::logger::LogLevel::Error) {
            $crate::logger::log_error($module, &format!($($arg)*));
        }
    };
}