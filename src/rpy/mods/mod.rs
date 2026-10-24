//! pygame 子模块绑定（每模块一文件，2026-10-24 自 native_pygame.rs 拆分；
//! body 原样搬运，头按 standalone pymodule 形态重建——见 CLAUDE.md 批次 43）。
//! 各文件尾 `pub(crate) use xxx::module_def;` 供 registry 挂载取用。
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
pub(crate) mod dialog;
pub(crate) mod display;
#[cfg(feature = "gui")]
pub(crate) mod egui;
pub(crate) mod event;
pub(crate) mod font;
pub(crate) mod gamepad;
pub(crate) mod gfxdraw;
pub(crate) mod image;
pub(crate) mod io;
#[cfg(feature = "physics2d")]
pub(crate) mod rapier;
pub(crate) mod key;
pub(crate) mod mask;
pub(crate) mod math;
pub(crate) mod mixer;
pub(crate) mod mouse;
pub(crate) mod net;
pub(crate) mod permission;
pub(crate) mod sndarray;
pub(crate) mod sprite;
pub(crate) mod time;
pub(crate) mod touch;
pub(crate) mod transform;
pub(crate) mod version;
pub(crate) mod video;

