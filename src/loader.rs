use libloading::{Library, Symbol};
use napi::{Error, Status};
use once_cell::sync::OnceCell;
use std::os::raw::{c_char, c_int, c_long, c_uint, c_void};

use crate::libpath::get_lib_path;

// 定义curl库的类型别名
pub type CurlHandle = *mut std::ffi::c_void;
pub type CurlMultiHandle = *mut std::ffi::c_void;
pub type CurlSlist = *mut std::ffi::c_void;
pub type CurlMime = *mut std::ffi::c_void;
pub type CurlMimepart = *mut std::ffi::c_void;

// 存储加载的函数实例
static CURL_FUNCTIONS: OnceCell<CurlFunctions> = OnceCell::new();

// 回调函数类型
pub type WriteCallback = unsafe extern "C" fn(
  contents: *mut c_char,
  size: usize,
  nmemb: usize,
  userp: *mut c_void,
) -> usize;

pub type ReadCallback = unsafe extern "C" fn(
  buffer: *mut c_char,
  size: usize,
  nitems: usize,
  userp: *mut c_void,
) -> usize;

pub type ProgressCallback = unsafe extern "C" fn(
  clientp: *mut c_void,
  dltotal: f64,
  dlnow: f64,
  ultotal: f64,
  ulnow: f64,
) -> c_int;

pub type HeaderCallback = unsafe extern "C" fn(
  buffer: *mut c_char,
  size: usize,
  nitems: usize,
  userdata: *mut c_void,
) -> usize;

// Easy interface 函数类型
pub type CurlEasyInit = unsafe extern "C" fn() -> CurlHandle;
pub type CurlEasyCleanup = unsafe extern "C" fn(handle: CurlHandle);
pub type CurlEasySetopt =
  unsafe extern "C" fn(handle: CurlHandle, option: c_int, value: *const c_void) -> c_int;
pub type CurlEasyPerform = unsafe extern "C" fn(handle: CurlHandle) -> c_int;
pub type CurlEasyGetinfo =
  unsafe extern "C" fn(handle: CurlHandle, info: c_int, value: *mut c_void) -> c_int;
pub type CurlEasyDuphandle = unsafe extern "C" fn(handle: CurlHandle) -> CurlHandle;
pub type CurlEasyReset = unsafe extern "C" fn(handle: CurlHandle);
pub type CurlEasyStrerror = unsafe extern "C" fn(code: c_int) -> *const c_char;
// 添加 impersonate 函数类型
pub type CurlEasyImpersonate =
  unsafe extern "C" fn(handle: CurlHandle, target: *const c_char) -> c_int;

// Multi interface 函数类型
pub type CurlMultiInit = unsafe extern "C" fn() -> CurlMultiHandle;
pub type CurlMultiCleanup = unsafe extern "C" fn(handle: CurlMultiHandle) -> c_int;
pub type CurlMultiPerform =
  unsafe extern "C" fn(multi_handle: CurlMultiHandle, running_handles: *mut c_int) -> c_int;
pub type CurlMultiWait = unsafe extern "C" fn(
  multi_handle: CurlMultiHandle,
  extra_fds: *mut c_void,
  extra_nfds: c_uint,
  timeout_ms: c_int,
  ret: *mut c_int,
) -> c_int;
pub type CurlMultiAddHandle =
  unsafe extern "C" fn(multi_handle: CurlMultiHandle, easy_handle: CurlHandle) -> c_int;
pub type CurlMultiRemoveHandle =
  unsafe extern "C" fn(multi_handle: CurlMultiHandle, easy_handle: CurlHandle) -> c_int;
pub type CurlMultiInfoRead =
  unsafe extern "C" fn(multi_handle: CurlMultiHandle, msgs_in_queue: *mut c_int) -> *mut c_void;
pub type CurlMultiSetopt =
  unsafe extern "C" fn(multi_handle: CurlMultiHandle, option: c_int, value: *const c_void) -> c_int;
pub type CurlMultiStrerror = unsafe extern "C" fn(code: c_int) -> *const c_char;

// Slist 函数类型
pub type CurlSlistAppend = unsafe extern "C" fn(list: CurlSlist, data: *const c_char) -> CurlSlist;
pub type CurlSlistFreeAll = unsafe extern "C" fn(list: CurlSlist);

// MIME 函数类型
pub type CurlMimeInit = unsafe extern "C" fn(easy: CurlHandle) -> CurlMime;
pub type CurlMimeFree = unsafe extern "C" fn(mime: CurlMime);
pub type CurlMimeAddpart = unsafe extern "C" fn(mime: CurlMime) -> CurlMimepart;
pub type CurlMimeName = unsafe extern "C" fn(part: CurlMimepart, name: *const c_char) -> c_int;
pub type CurlMimeData =
  unsafe extern "C" fn(part: CurlMimepart, data: *const c_char, datasize: usize) -> c_int;
pub type CurlMimeFiledata =
  unsafe extern "C" fn(part: CurlMimepart, filename: *const c_char) -> c_int;
pub type CurlMimeFilename =
  unsafe extern "C" fn(part: CurlMimepart, filename: *const c_char) -> c_int;
pub type CurlMimeType = unsafe extern "C" fn(part: CurlMimepart, mimetype: *const c_char) -> c_int;

// URL API 函数类型
pub type CurlUrlCleanup = unsafe extern "C" fn(handle: *mut c_void);
pub type CurlUrl = unsafe extern "C" fn() -> *mut c_void;
pub type CurlUrlSet = unsafe extern "C" fn(
  handle: *mut c_void,
  part: c_int,
  content: *const c_char,
  flags: c_uint,
) -> c_int;
pub type CurlUrlGet = unsafe extern "C" fn(
  handle: *mut c_void,
  part: c_int,
  content: *mut *mut c_char,
  flags: c_uint,
) -> c_int;

// 全局函数类型
pub type CurlGlobalInit = unsafe extern "C" fn(flags: c_long) -> c_int;
pub type CurlGlobalCleanup = unsafe extern "C" fn();
pub type CurlVersion = unsafe extern "C" fn() -> *const c_char;
pub type CurlVersionInfo = unsafe extern "C" fn(version: c_int) -> *mut c_void;
pub type CurlEscape = unsafe extern "C" fn(string: *const c_char, length: c_int) -> *mut c_char;
pub type CurlUnescape = unsafe extern "C" fn(string: *const c_char, length: c_int) -> *mut c_char;
pub type CurlFree = unsafe extern "C" fn(ptr: *mut c_void);

// 存储所有加载的函数
#[derive(Clone)]
pub struct CurlFunctions {
  // Easy interface
  pub easy_init: Symbol<'static, CurlEasyInit>,
  pub easy_cleanup: Symbol<'static, CurlEasyCleanup>,
  pub easy_setopt: Symbol<'static, CurlEasySetopt>,
  pub easy_perform: Symbol<'static, CurlEasyPerform>,
  pub easy_getinfo: Symbol<'static, CurlEasyGetinfo>,
  pub easy_duphandle: Symbol<'static, CurlEasyDuphandle>,
  pub easy_reset: Symbol<'static, CurlEasyReset>,
  pub easy_strerror: Symbol<'static, CurlEasyStrerror>,
  // 添加 impersonate 函数
  pub easy_impersonate: Symbol<'static, CurlEasyImpersonate>,

  // Multi interface
  pub multi_init: Symbol<'static, CurlMultiInit>,
  pub multi_cleanup: Symbol<'static, CurlMultiCleanup>,
  pub multi_perform: Symbol<'static, CurlMultiPerform>,
  pub multi_wait: Symbol<'static, CurlMultiWait>,
  pub multi_add_handle: Symbol<'static, CurlMultiAddHandle>,
  pub multi_remove_handle: Symbol<'static, CurlMultiRemoveHandle>,
  pub multi_info_read: Symbol<'static, CurlMultiInfoRead>,
  pub multi_setopt: Symbol<'static, CurlMultiSetopt>,
  pub multi_strerror: Symbol<'static, CurlMultiStrerror>,

  // Slist
  pub slist_append: Symbol<'static, CurlSlistAppend>,
  pub slist_free_all: Symbol<'static, CurlSlistFreeAll>,

  // MIME
  pub mime_init: Symbol<'static, CurlMimeInit>,
  pub mime_free: Symbol<'static, CurlMimeFree>,
  pub mime_addpart: Symbol<'static, CurlMimeAddpart>,
  pub mime_name: Symbol<'static, CurlMimeName>,
  pub mime_data: Symbol<'static, CurlMimeData>,
  pub mime_filedata: Symbol<'static, CurlMimeFiledata>,
  pub mime_filename: Symbol<'static, CurlMimeFilename>,
  pub mime_type: Symbol<'static, CurlMimeType>,

  // URL API
  pub url_cleanup: Symbol<'static, CurlUrlCleanup>,
  pub url: Symbol<'static, CurlUrl>,
  pub url_set: Symbol<'static, CurlUrlSet>,
  pub url_get: Symbol<'static, CurlUrlGet>,

  // Global functions
  pub global_init: Symbol<'static, CurlGlobalInit>,
  pub global_cleanup: Symbol<'static, CurlGlobalCleanup>,
  pub version: Symbol<'static, CurlVersion>,
  pub version_info: Symbol<'static, CurlVersionInfo>,
  pub escape: Symbol<'static, CurlEscape>,
  pub unescape: Symbol<'static, CurlUnescape>,
  pub free: Symbol<'static, CurlFree>,
}

// 实现 Send 和 Sync trait
unsafe impl Send for CurlFunctions {}
unsafe impl Sync for CurlFunctions {}

// 加载lib方法
pub fn load_curl_library() -> Result<&'static CurlFunctions, Box<dyn std::error::Error>> {
  let lib_path = get_lib_path().ok_or("lib path is not set")?;

  CURL_FUNCTIONS.get_or_try_init(|| {
    let lib = unsafe { Library::new(&lib_path)? };
    let lib_static: &'static Library = Box::leak(Box::new(lib));

    let functions = CurlFunctions {
      // Easy interface
      easy_init: unsafe { lib_static.get(b"curl_easy_init\0")? },
      easy_cleanup: unsafe { lib_static.get(b"curl_easy_cleanup\0")? },
      easy_setopt: unsafe { lib_static.get(b"curl_easy_setopt\0")? },
      easy_perform: unsafe { lib_static.get(b"curl_easy_perform\0")? },
      easy_getinfo: unsafe { lib_static.get(b"curl_easy_getinfo\0")? },
      easy_duphandle: unsafe { lib_static.get(b"curl_easy_duphandle\0")? },
      easy_reset: unsafe { lib_static.get(b"curl_easy_reset\0")? },
      easy_strerror: unsafe { lib_static.get(b"curl_easy_strerror\0")? },
      easy_impersonate: unsafe { lib_static.get(b"curl_easy_impersonate\0")? },

      // Multi interface
      multi_init: unsafe { lib_static.get(b"curl_multi_init\0")? },
      multi_cleanup: unsafe { lib_static.get(b"curl_multi_cleanup\0")? },
      multi_perform: unsafe { lib_static.get(b"curl_multi_perform\0")? },
      multi_wait: unsafe { lib_static.get(b"curl_multi_wait\0")? },
      multi_add_handle: unsafe { lib_static.get(b"curl_multi_add_handle\0")? },
      multi_remove_handle: unsafe { lib_static.get(b"curl_multi_remove_handle\0")? },
      multi_info_read: unsafe { lib_static.get(b"curl_multi_info_read\0")? },
      multi_setopt: unsafe { lib_static.get(b"curl_multi_setopt\0")? },
      multi_strerror: unsafe { lib_static.get(b"curl_multi_strerror\0")? },

      // Slist
      slist_append: unsafe { lib_static.get(b"curl_slist_append\0")? },
      slist_free_all: unsafe { lib_static.get(b"curl_slist_free_all\0")? },

      // MIME
      mime_init: unsafe { lib_static.get(b"curl_mime_init\0")? },
      mime_free: unsafe { lib_static.get(b"curl_mime_free\0")? },
      mime_addpart: unsafe { lib_static.get(b"curl_mime_addpart\0")? },
      mime_name: unsafe { lib_static.get(b"curl_mime_name\0")? },
      mime_data: unsafe { lib_static.get(b"curl_mime_data\0")? },
      mime_filedata: unsafe { lib_static.get(b"curl_mime_filedata\0")? },
      mime_filename: unsafe { lib_static.get(b"curl_mime_filename\0")? },
      mime_type: unsafe { lib_static.get(b"curl_mime_type\0")? },

      // URL API
      url_cleanup: unsafe { lib_static.get(b"curl_url_cleanup\0")? },
      url: unsafe { lib_static.get(b"curl_url\0")? },
      url_set: unsafe { lib_static.get(b"curl_url_set\0")? },
      url_get: unsafe { lib_static.get(b"curl_url_get\0")? },

      // Global functions
      global_init: unsafe { lib_static.get(b"curl_global_init\0")? },
      global_cleanup: unsafe { lib_static.get(b"curl_global_cleanup\0")? },
      version: unsafe { lib_static.get(b"curl_version\0")? },
      version_info: unsafe { lib_static.get(b"curl_version_info\0")? },
      escape: unsafe { lib_static.get(b"curl_escape\0")? },
      unescape: unsafe { lib_static.get(b"curl_unescape\0")? },
      free: unsafe { lib_static.get(b"curl_free\0")? },
    };

    Ok(functions)
  })
}

// 获取已加载的函数实例（如果已经加载）
pub fn get_curl_functions() -> Option<&'static CurlFunctions> {
  CURL_FUNCTIONS.get()
}

// 检查库是否已加载
pub fn is_library_loaded() -> bool {
  CURL_FUNCTIONS.get().is_some()
}

pub fn napi_load_library() -> napi::Result<&'static CurlFunctions> {
  load_curl_library().or_else(|_| {
    Err(Error::new(
      Status::GenericFailure,
      "Failed to load curl-impersonate library",
    ))
  })
}
