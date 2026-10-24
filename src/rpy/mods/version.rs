//! pygame.version 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod version {
        use rustpython_vm::VirtualMachine;

        /// pygame.version.ver：starfish pygame 层版本（= crate 版本）
        #[pyattr(name = "ver")]
        const VER: &str = starfish_pygame::version::VERSION;

        /// pygame.version.vernum：版本三元组
        #[pyfunction]
        fn get_version() -> (u8, u8, u8) {
            starfish_pygame::version::get_version()
        }
    }

pub(crate) use version::module_def;
