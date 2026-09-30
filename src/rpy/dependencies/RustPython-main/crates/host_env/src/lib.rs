#![allow(clippy::must_use_candidate)]

extern crate alloc;

#[macro_use]
mod macros;
pub use macros::*;

// [pygame-rs 补丁②] ctypes 引用 libffi（补丁①已从 android 目标段剔除），
// 模块必须一并门控，否则 android 编译失败。功能零损失：本引擎嵌入
// 未启用 host_env 特性，stdlib 的 _ctypes 本就不注册（所有平台一致）。
#[cfg(not(target_os = "android"))]
pub mod ctypes;
#[cfg(any(unix, windows, target_os = "wasi"))]
pub mod errno;
#[cfg(any(unix, windows, target_os = "wasi"))]
pub mod io;
#[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
#[path = "io_unsupported.rs"]
pub mod io;
pub mod os;
#[cfg(any(unix, windows))]
pub mod thread;

#[cfg(any(unix, windows, target_os = "wasi"))]
pub mod crt_fd;
#[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
#[path = "crt_fd_unsupported.rs"]
pub mod crt_fd;

#[cfg(any(not(target_arch = "wasm32"), target_os = "wasi"))]
pub mod fileutils;
pub mod fs;
#[cfg(any(unix, windows))]
pub mod locale;
#[cfg(feature = "native-certs")]
pub mod native_certs;
pub mod readline;
#[cfg(feature = "ssl")]
pub mod ssl;

#[cfg(windows)]
pub mod windows;

#[cfg(any(unix, target_os = "wasi"))]
pub mod fcntl;
#[cfg(any(unix, windows, target_os = "wasi"))]
pub mod select;
#[cfg(any(unix, windows))]
pub mod socket;
#[cfg(unix)]
pub mod syslog;
#[cfg(all(unix, not(target_os = "redox"), not(target_os = "ios")))]
pub mod termios;

#[cfg(unix)]
pub mod grp;
#[cfg(unix)]
pub mod posix;
#[cfg(target_os = "wasi")]
#[path = "posix_wasi.rs"]
pub mod posix;
#[cfg(windows)]
#[path = "posix_windows.rs"]
pub mod posix;
#[cfg(any(unix, target_os = "wasi"))]
pub mod posix_unix_like;
#[cfg(unix)]
pub mod pwd;
#[cfg(unix)]
pub mod resource;
#[cfg(all(unix, not(target_os = "redox"), not(target_os = "android")))]
pub mod shm;
#[cfg(any(unix, windows))]
pub mod signal;
pub mod time;

#[cfg(windows)]
pub mod cert_store;
#[cfg(target_os = "macos")]
pub mod system_configuration {
    pub use ::system_configuration::*;
}
#[cfg(any(unix, windows))]
pub mod faulthandler;
#[cfg(any(unix, windows))]
pub mod mmap;
#[cfg(windows)]
pub mod msvcrt;
#[cfg(any(unix, windows))]
pub mod multiprocessing;
#[cfg(windows)]
pub mod nt;
#[cfg(windows)]
pub mod overlapped;
#[cfg(windows)]
pub mod testconsole;
#[cfg(windows)]
pub mod uuid;
#[cfg(windows)]
pub mod winapi;
#[cfg(windows)]
pub mod winreg;
#[cfg(windows)]
pub mod winsound;
#[cfg(windows)]
pub mod wmi;
