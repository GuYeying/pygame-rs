//! pygame.display 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod display {
        use crate::rpy::pygame::pygame::PyScreen;
        use crate::rpy::bridge;
        use crate::rpy::args::SfPoint;
        use rustpython_derive::FromArgs;
        use rustpython_vm::convert::TryFromObject;
        use rustpython_vm::{
            PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine,
            builtins::PyStrRef,
        };

        // ── 窗口旗标（locals::display 透传；set_mode 当前收下不解析，前向兼容）──
        #[pyattr]
        pub(crate) const RESIZABLE: u32 = starfish_pygame::display::RESIZABLE;
        #[pyattr]
        pub(crate) const FULLSCREEN: u32 = starfish_pygame::display::FULLSCREEN;
        #[pyattr]
        pub(crate) const NOFRAME: u32 = starfish_pygame::display::NOFRAME;
        #[pyattr]
        pub(crate) const HIDDEN: u32 = starfish_pygame::display::HIDDEN;
        #[pyattr]
        pub(crate) const ALWAYS_ON_TOP: u32 = starfish_pygame::display::ALWAYS_ON_TOP;
        #[pyattr]
        pub(crate) const TRANSPARENT: u32 = starfish_pygame::display::TRANSPARENT;
        #[pyattr]
        pub(crate) const VULKAN: u32 = starfish_pygame::display::VULKAN;
        #[pyattr]
        pub(crate) const DIRECTX: u32 = starfish_pygame::display::DIRECTX;
        #[pyattr]
        pub(crate) const METAL: u32 = starfish_pygame::display::METAL;
        #[pyattr]
        pub(crate) const OPENGLES: u32 = starfish_pygame::display::OPENGLES;

        /// set_mode(size, flags=0) → Screen 占位（真身由宿主壳先行装配，
        /// 此处记录需求；flags 批次 O 落地——位集/序列翻译到运行时窗口
        /// 旗标（apply_window_flags：RESIZABLE/FULLSCREEN/NOFRAME 即时
        /// 生效，创建期旗标垫底收下）。
        #[derive(FromArgs)]
        struct SetModeArgs {
            #[pyarg(any)]
            size: SfPoint,
            #[pyarg(any, optional)]
            flags: Option<PyObjectRef>,
        }

        /// flags 实参 → 位集（int 位掩码或 [位, ...] 序列，pygame 两形态）
        fn flags_bits(flags: &PyObjectRef, vm: &VirtualMachine) -> PyResult<u32> {
            if let Ok(bits) = u32::try_from_object(vm, flags.clone()) {
                return Ok(bits);
            }
            let parts: Vec<u32> = vm.extract_elements_with(flags, |o| {
                u32::try_from_object(vm, o.clone())
                    .map_err(|_| vm.new_type_error("flags 项应为 int 旗标位"))
            })?;
            Ok(parts.into_iter().fold(0u32, |acc, b| acc | b))
        }

        #[pyfunction]
        fn set_mode(args: SetModeArgs, vm: &VirtualMachine) -> PyResult<PyRef<PyScreen>> {
            // 启动诊断（单行打点：脚本 set_mode 的落地耗时）
            let t = starfish_std::debug::Stopwatch::now();
            // 完整窗口请求（批次 O + 尺寸扩展）：尺寸跟随（set_size）+
            // 旗标运行时生效（apply_window_request）
            let bits = match &args.flags {
                Some(f) => flags_bits(f, vm)?,
                None => 0,
            };
            bridge::devlog(&format!(
                "[pygame] display.set_mode {:?} 旗标 {bits:#b}（真身由宿主壳装配）",
                (args.size.0, args.size.1)
            ));
            starfish_pygame::display::apply_window_request(
                (args.size.0.max(0) as u32, args.size.1.max(0) as u32),
                bits,
            );
            bridge::devlog(&format!(
                "[set_mode] 落地 {}ms",
                t.ms()
            ));
            Ok(PyScreen.into_ref(&vm.ctx))
        }

        #[pyfunction]
        fn flip(_vm: &VirtualMachine) {
            starfish_pygame::display::flip();
        }

        /// set_opacity(v)：窗口整体不透明度（0.0 透明 .. 1.0 不透明）。
        /// 桌宠/悬浮件形态；与 TRANSPARENT 创建旗标正交（后者让 alpha 0
        /// 区域透出桌面，前者调窗口整体 alpha）。
        #[pyfunction]
        fn set_opacity(opacity: f32, _vm: &VirtualMachine) {
            starfish_pygame::display::set_opacity(opacity)
        }

        /// set_caption(title)：窗口标题。
        #[pyfunction]
        fn set_caption(title: PyStrRef, vm: &VirtualMachine) -> PyResult<()> {
            let t = title
                .to_str()
                .ok_or_else(|| vm.new_value_error("标题含非 UTF-8 字符"))?;
            starfish_pygame::display::set_caption(t);
            Ok(())
        }

        /// get_init() -> bool：set_mode 是否已调用（display 初始化态）。
        #[pyfunction]
        fn get_init(_vm: &VirtualMachine) -> bool {
            starfish_pygame::display::get_init()
        }
    }

pub(crate) use display::module_def;
