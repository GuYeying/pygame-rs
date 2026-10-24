//! pygame.time 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod time {
        use rustpython_derive::pyclass;
        use rustpython_vm::types::Constructor;
        use rustpython_vm::{
            Py, PyPayload, PyRef, PyResult, VirtualMachine, builtins::PyType,
        };

        #[pyfunction]
        fn get_ticks(_vm: &VirtualMachine) -> u64 {
            starfish_time::since_start_ms()
        }

        /// pygame time.delay（Web 上 no-op——无阻塞模型，见 pygame/time.rs）。
        #[pyfunction]
        fn delay(ms: u64, _vm: &VirtualMachine) {
            starfish_pygame::time::delay(ms);
        }

        // ── Clock：帧时钟（上限节流语义——详见 pygame/time.rs）────────
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.time", name = "Clock", unhashable = true)]
        pub(crate) struct PyClock {
            pub(crate) inner: std::cell::RefCell<starfish_pygame::time::Clock>,
        }

        impl std::fmt::Debug for PyClock {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("Clock(<frame>)")
            }
        }

        #[pyclass(with(Constructor))]
        impl PyClock {
            /// tick(fps=0) -> float：推进一帧 + 上限节流，返回本帧真实
            /// 间隔（**毫秒**，pygame 语义；引擎层为秒——绑定位换算）。
            /// vsync 已把帧距拉到 ≥ 目标时不再补睡（三端同语义，无双重
            /// 节流）；Web 上睡眠 no-op（rAF 节流），本调用只作度量。
            #[pymethod]
            fn tick(&self, fps: Option<u32>) -> f32 {
                self.inner.borrow_mut().tick(fps.unwrap_or(0)) * 1000.0
            }

            /// get_time() -> float：本帧真实间隔（毫秒，同 tick 返回值）。
            #[pymethod]
            fn get_time(&self) -> f32 {
                self.inner.borrow().raw_delta() * 1000.0
            }

            /// get_fps() -> float：EMA 平滑帧率。
            #[pymethod]
            fn get_fps(&self) -> f32 {
                self.inner.borrow().get_fps()
            }
        }

        impl Constructor for PyClock {
            type Args = ();

            fn py_new(
                _cls: &Py<PyType>,
                _args: Self::Args,
                _vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self {
                    inner: std::cell::RefCell::new(starfish_pygame::time::Clock::new()),
                })
            }
        }
    }

pub(crate) use time::module_def;
