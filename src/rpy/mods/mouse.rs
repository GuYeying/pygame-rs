//! pygame.mouse 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod mouse {
        use crate::rpy::args::SfPoint;
        use rustpython_vm::VirtualMachine;

        /// 鼠标位置（客户区物理像素，y 向下；事件泵前为 (0, 0)）。
        #[pyfunction]
        fn get_pos(_vm: &VirtualMachine) -> (i32, i32) {
            starfish_pygame::mouse::get_pos()
        }

        /// 左/中/右三键按下态（pygame 惯例）。
        #[pyfunction]
        fn get_pressed(_vm: &VirtualMachine) -> (bool, bool, bool) {
            starfish_pygame::mouse::get_pressed()
        }

        /// 相对位移（自上次 get_rel 调用起；连续调用逐段累加）。
        #[pyfunction]
        fn get_rel(_vm: &VirtualMachine) -> (i32, i32) {
            starfish_pygame::mouse::get_rel()
        }

        /// 光标可见态（窗口层 set_cursor_visible；移动端无光标语义）。
        #[pyfunction]
        fn set_visible(visible: bool, _vm: &VirtualMachine) {
            starfish_pygame::mouse::set_visible(visible)
        }

        /// 光标可见态查询。
        #[pyfunction]
        fn get_visible(_vm: &VirtualMachine) -> bool {
            starfish_pygame::mouse::get_visible()
        }

        /// 窗口鼠标聚焦态（简化：与键盘聚焦同源）。
        #[pyfunction]
        fn get_focused(_vm: &VirtualMachine) -> bool {
            starfish_pygame::mouse::get_focused()
        }

        /// warp 光标（垫底：仅更新状态表，不扭曲真实光标、无合成事件
        /// ——starfish-window v1 无光标扭曲能力）。
        #[pyfunction]
        fn set_pos(pos: SfPoint, _vm: &VirtualMachine) {
            starfish_pygame::mouse::set_pos(pos.0, pos.1)
        }
    }

    // ── gamepad：手柄查询（base gamepad 状态表，宿主泵每帧排水）──────
    // 命名定约（架构文档 §二）：替代 joystick+controller 两名，按钮语义
    // 对齐 SDL/标准布局。常量 = 整数索引（pygame.controller 同惯例；
    // Web Gamepad API 同序，映射表 = starfish BUTTON_ORDER/AXIS_ORDER，

pub(crate) use mouse::module_def;
