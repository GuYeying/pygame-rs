//! pygame.rapier 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod rapier {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        use rapier_starfish_debug::dim2::debug::DebugDraw as SfDebugDraw;
        use rapier_starfish_debug::dim2::debug::LineSink as SfLineSink;
        use rapier_starfish_debug::dim2::prelude::{
            ColliderBuilder as SfColliderBuilder, RigidBodyBuilder as SfRigidBodyBuilder,
        };
        use rapier_starfish_debug::dim2::world::PhysicsWorld as SfPhysicsWorld;
        use rustpython_derive::pyclass;
        use rustpython_vm::{PyPayload, PyRef, PyResult, VirtualMachine};

        use crate::rpy::pyrapier::RendererGpu;

        /// 物理世界（rapier `PhysicsWorld` 薄壳 + 固定步长积分器）
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.rapier", name = "PhysicsWorld", unhashable = true)]
        pub(crate) struct PyPhysicsWorld {
            pub(crate) inner: Rc<RefCell<SfPhysicsWorld>>,
        }

        impl std::fmt::Debug for PyPhysicsWorld {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&format!("PhysicsWorld(<sim_time={}>)", self.inner.borrow().sim_time()))
            }
        }

        /// 刚体 builder（构造 kwargs 收参；insert 一次性消费）
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.rapier", name = "RigidBodyBuilder", unhashable = true)]
        pub(crate) struct PyRigidBodyBuilder {
            pub(crate) builder: RefCell<Option<SfRigidBodyBuilder>>,
        }

        impl std::fmt::Debug for PyRigidBodyBuilder {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("RigidBodyBuilder(<kwargs-built>)")
            }
        }

        /// 碰撞体 builder（构造 kwargs 收参；insert 一次性消费）
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.rapier", name = "ColliderBuilder", unhashable = true)]
        pub(crate) struct PyColliderBuilder {
            pub(crate) builder: RefCell<Option<SfColliderBuilder>>,
        }

        impl std::fmt::Debug for PyColliderBuilder {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("ColliderBuilder(<kwargs-built>)")
            }
        }

        /// 调试渲染控制面（内容位集 = rapier DebugRenderMode）
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.rapier", name = "DebugDraw", unhashable = true)]
        pub(crate) struct PyDebugDraw {
            pub(crate) inner: RefCell<SfDebugDraw>,
        }

        impl std::fmt::Debug for PyDebugDraw {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("DebugDraw(<mode-bits>)")
            }
        }

        /// 线段收集器（collect 的落点；每帧复用零分配）
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.rapier", name = "LineSink", unhashable = true)]
        pub(crate) struct PyLineSink {
            pub(crate) inner: RefCell<SfLineSink>,
        }

        impl std::fmt::Debug for PyLineSink {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&format!("LineSink(<{} lines>)", self.inner.borrow().line_count()))
            }
        }

        /// 调试线绘制器（GPU 惰性装配——首次 draw 需已 set_mode）
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.rapier", name = "DebugLinesRenderer", unhashable = true)]
        pub(crate) struct PyDebugLinesRenderer {
            pub(crate) ppm: Cell<f32>,
            pub(crate) state: RefCell<Option<RendererGpu>>,
        }

        impl std::fmt::Debug for PyDebugLinesRenderer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&format!("DebugLinesRenderer(<ppm={}>)", self.ppm.get()))
            }
        }

        /// collect(world, draw, sink)：物理状态 → 线段流（渲染前每帧调用）
        #[pyfunction]
        fn collect(
            world: PyRef<PyPhysicsWorld>,
            draw: PyRef<PyDebugDraw>,
            sink: PyRef<PyLineSink>,
            _vm: &VirtualMachine,
        ) -> PyResult<()> {
            crate::rpy::pyrapier::collect(world, draw, sink);
            Ok(())
        }
    }

// ── 文件级 re-export（外部引用归位）──
pub(crate) use rapier::{PyPhysicsWorld, PyRigidBodyBuilder, PyColliderBuilder, PyDebugDraw, PyLineSink, PyDebugLinesRenderer};
#[cfg(feature = "physics2d")]
pub(crate) use rapier::module_def;
