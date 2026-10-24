//! pygame.touch 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod touch {
        use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine};

        /// 当前按住的手指数（0 = 无触摸；桌面恒 0）。
        #[pyfunction]
        fn get_count(_vm: &VirtualMachine) -> usize {
            starfish_pygame::touch::get_count()
        }

        /// 活动手指快照：[{"id", "x", "y", "phase"}] 按 id 升序。
        /// phase 仅 "started"/"moved"——抬起的手指即出表（Ended/
        /// Cancelled 不驻留）。
        #[pyfunction]
        fn get_fingers(vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let mut items: Vec<PyObjectRef> = Vec::new();
            for f in starfish_pygame::touch::fingers() {
                let d = vm.ctx.new_dict();
                d.set_item("id", vm.ctx.new_int(f.id as i64).into(), vm)?;
                d.set_item("x", vm.ctx.new_int(f.x as i64).into(), vm)?;
                d.set_item("y", vm.ctx.new_int(f.y as i64).into(), vm)?;
                let phase = match f.phase {
                    starfish_we::TouchPhase::Started => "started",
                    starfish_we::TouchPhase::Moved => "moved",
                    _ => "unknown",
                };
                d.set_item("phase", vm.ctx.new_str(phase).into(), vm)?;
                items.push(d.into());
            }
            Ok(vm.ctx.new_list(items).into())
        }

        /// 指定手指的位置 (x, y)（客户区物理像素，y 向下）；无该手指 = None。
        #[pyfunction]
        fn get_pos(finger_id: u64, _vm: &VirtualMachine) -> Option<(i32, i32)> {
            starfish_pygame::touch::get_pos(finger_id)
        }
    }

    // ── io：文件读写（批次 M；原生同步直读，含基准目录语义——
    // Android = 引擎 run 自动注入的应用私有目录，绝对路径原样）──
    // Web v1 不提供同步面（fetch 真异步，Python 同步面无法承载——

pub(crate) use touch::module_def;
