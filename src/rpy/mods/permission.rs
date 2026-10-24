//! pygame.permission 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
#![allow(unused_imports)]
use rustpython_derive::{pymodule, pyclass, FromArgs};
use crate::rpy::{bridge, args};
use crate::rpy::pygame::pygame::{PySurface, PySurfaceSession, PyScreen, PyScreenSession, PyColor};
use rustpython_vm::{
    PyObjectRef, Py, PyPayload, PyRef, PyResult, VirtualMachine,
    function::{FuncArgs, PyComparisonValue},
    types::{Comparable, Constructor, PyComparisonOp, Representable},
    builtins::{PyModule, PyStr, PyStrRef, PyType},
};
use std::cell::Cell;
#[pymodule]
    pub mod permission {
        use rustpython_vm::{PyResult, VirtualMachine};
        use starfish_pygame::permission::{self as sf_perm, Permission};

        /// 权限标识：pygame.permission.MICROPHONE / INTERNET
        #[pyattr]
        pub(crate) const MICROPHONE: i32 = 0;
        #[pyattr]
        pub(crate) const INTERNET: i32 = 1;

        fn map_perm(p: i32, vm: &VirtualMachine) -> PyResult<Permission> {
            match p {
                MICROPHONE => Ok(Permission::Microphone),
                INTERNET => Ok(Permission::Internet),
                _ => Err(vm.new_value_error(format!(
                    "未知权限标识 {p}（可用：pygame.permission.MICROPHONE / INTERNET）"
                ))),
            }
        }

        /// ensure(permission) -> bool：同步申请（Android MICROPHONE 会
        /// 弹系统授权框**阻塞等待**——自动化冒烟勿触碰；INTERNET 瞬时）。
        #[pyfunction]
        fn ensure(p: i32, vm: &VirtualMachine) -> PyResult<bool> {
            Ok(sf_perm::ensure(map_perm(p, vm)?))
        }
    }

    // ── video：视频解码（六平台硬解；接口面——真实播放的呈现集成

pub(crate) use permission::module_def;
