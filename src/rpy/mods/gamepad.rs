//! pygame.gamepad 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod gamepad {
        use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine};

        use starfish_gamepad::{Axis, Button};

        // ── 按钮常量（标准布局索引；pygame.controller 同惯例）──
        #[pyattr]
        pub(crate) const BTN_SOUTH: u8 = 0; // A（下侧）
        #[pyattr]
        pub(crate) const BTN_EAST: u8 = 1; // B（右侧）
        #[pyattr]
        pub(crate) const BTN_WEST: u8 = 2; // X（左侧）
        #[pyattr]
        pub(crate) const BTN_NORTH: u8 = 3; // Y（上侧）
        #[pyattr]
        pub(crate) const BTN_LB: u8 = 4; // 左肩键
        #[pyattr]
        pub(crate) const BTN_RB: u8 = 5; // 右肩键
        #[pyattr]
        pub(crate) const BTN_LT: u8 = 6; // 左扳机
        #[pyattr]
        pub(crate) const BTN_RT: u8 = 7; // 右扳机
        #[pyattr]
        pub(crate) const BTN_SELECT: u8 = 8;
        #[pyattr]
        pub(crate) const BTN_START: u8 = 9;
        #[pyattr]
        pub(crate) const BTN_LTHUMB: u8 = 10; // 左摇杆按下
        #[pyattr]
        pub(crate) const BTN_RTHUMB: u8 = 11; // 右摇杆按下
        #[pyattr]
        pub(crate) const BTN_DPAD_UP: u8 = 12;
        #[pyattr]
        pub(crate) const BTN_DPAD_DOWN: u8 = 13;
        #[pyattr]
        pub(crate) const BTN_DPAD_LEFT: u8 = 14;
        #[pyattr]
        pub(crate) const BTN_DPAD_RIGHT: u8 = 15;

        // ── 轴常量（0..3 = Web 标准四轴；4/5 = 左/右扳机行程）──
        #[pyattr]
        pub(crate) const AXIS_LX: u8 = 0;
        #[pyattr]
        pub(crate) const AXIS_LY: u8 = 1;
        #[pyattr]
        pub(crate) const AXIS_RX: u8 = 2;
        #[pyattr]
        pub(crate) const AXIS_RY: u8 = 3;
        #[pyattr]
        pub(crate) const AXIS_LT: u8 = 4; // 左扳机（0..=1.0）
        #[pyattr]
        pub(crate) const AXIS_RT: u8 = 5; // 右扳机（0..=1.0）

        /// 索引 → Button（越界 ValueError）
        fn map_btn(i: u8, vm: &VirtualMachine) -> PyResult<Button> {
            Button::from_index(i)
                .ok_or_else(|| vm.new_value_error(format!("无效手柄按钮索引: {i}（0..16）")))
        }

        /// 索引 → Axis（越界 ValueError）
        fn map_axis(i: u8, vm: &VirtualMachine) -> PyResult<Axis> {
            Axis::from_index(i)
                .ok_or_else(|| vm.new_value_error(format!("无效手柄轴索引: {i}（0..6）")))
        }

        /// 已连接手柄数（0 = 无手柄）。
        #[pyfunction]
        fn get_count(_vm: &VirtualMachine) -> usize {
            starfish_pygame::gamepad::get_count()
        }

        /// 已连接手柄编号列表（连接顺序；平台内自增）。
        #[pyfunction]
        fn connected(vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let ids: Vec<PyObjectRef> = starfish_pygame::gamepad::connected()
                .into_iter()
                .map(|id| vm.ctx.new_int(id as i64).into())
                .collect();
            Ok(vm.ctx.new_list(ids).into())
        }

        /// 第一个连接的手柄编号（单手柄便捷入口；无 = None）。
        #[pyfunction]
        fn primary(_vm: &VirtualMachine) -> Option<usize> {
            starfish_pygame::gamepad::primary()
        }

        /// 指定手柄是否已连接。
        #[pyfunction]
        fn is_connected(id: usize, _vm: &VirtualMachine) -> bool {
            starfish_pygame::gamepad::is_connected(id)
        }

        /// 按键按下态（未连接 = False）。
        #[pyfunction]
        fn get_pressed(id: usize, btn: u8, vm: &VirtualMachine) -> PyResult<bool> {
            Ok(starfish_pygame::gamepad::is_pressed(id, map_btn(btn, vm)?))
        }

        /// 本帧新按下（上帧未按下；未连接 = False）。
        #[pyfunction]
        fn just_pressed(id: usize, btn: u8, vm: &VirtualMachine) -> PyResult<bool> {
            Ok(starfish_pygame::gamepad::just_pressed(id, map_btn(btn, vm)?))
        }

        /// 轴值（摇杆 -1.0..=1.0；扳机 0..=1.0；未连接 = 0.0）。
        #[pyfunction]
        fn get_axis(id: usize, axis: u8, vm: &VirtualMachine) -> PyResult<f32> {
            Ok(starfish_pygame::gamepad::axis(id, map_axis(axis, vm)?))
        }
    }

    // ── touch：多点触控手指查询（批次十三手指状态表的 Python 面）──────
    // 定约差异：pygame 官方按"设备→手指"两级编号（SDL 触屏设备概念），
    // 本层按**手指 id** 单层组织（现代移动惯例，多点语义直接可用——
    // starfish touch.rs 既定决策）。桌面端恒空表（仅移动端产生 Touch

pub(crate) use gamepad::module_def;
