//! pygame_physics —— 独立顶层 Python 模块（2026-10-24 架构抽象：自
//! `pygame.rapier` 平级升格；import pygame_physics 直达）。**单模块承载
//! 双维度**（2026-10-24 修订，用户定约）：2D 类面随 feature "physics2d"
//! （无后缀——`PhysicsWorld` 等），3D 类面随 feature "physics3d"
//! （`PhysicsWorld3D` 等后缀区分）；可双开，同名类分属不同后缀互不冲突，
//! 与 rapier-starfish-debug dim2/dim3 镜像面一一对应。
//! `runtime_2d.rs` / `runtime_3d.rs` = 各维度会话实现本体；本文件 =
//! #[pymodule] 声明 + 类型。文件夹形态 = 新模块效仿模板。
#![allow(unused_imports)]
use rustpython_derive::{pymodule, pyclass, FromArgs};
use crate::rpy::{bridge, args};
use rustpython_vm::{
    PyObjectRef, Py, PyPayload, PyRef, PyResult, VirtualMachine,
    builtins::{PyModule, PyStr, PyStrRef, PyType},
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[cfg(feature = "physics2d")]
pub(crate) mod runtime_2d;
#[cfg(feature = "physics3d")]
pub(crate) mod runtime_3d;

#[pymodule(name = "pygame_physics")]
    pub mod pygame_physics {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        #[cfg(feature = "physics2d")]
        use rapier_starfish_debug::dim2::debug::{DebugDraw as SfDebugDraw, LineSink as SfLineSink};
        #[cfg(feature = "physics2d")]
        use rapier_starfish_debug::dim2::prelude::{
            ColliderBuilder as SfColliderBuilder, RigidBodyBuilder as SfRigidBodyBuilder,
        };
        #[cfg(feature = "physics2d")]
        use rapier_starfish_debug::dim2::world::PhysicsWorld as SfPhysicsWorld;
        #[cfg(feature = "physics3d")]
        use rapier_starfish_debug::dim3::debug::{DebugDraw as SfDebugDraw3, LineSink as SfLineSink3};
        #[cfg(feature = "physics3d")]
        use rapier_starfish_debug::dim3::prelude::{
            ColliderBuilder as SfColliderBuilder3, RigidBodyBuilder as SfRigidBodyBuilder3,
        };
        #[cfg(feature = "physics3d")]
        use rapier_starfish_debug::dim3::world::PhysicsWorld as SfPhysicsWorld3;
        use rustpython_derive::pyclass;
        use rustpython_vm::{PyPayload, PyRef, PyResult, VirtualMachine};

        // ── 2D 类面（feature "physics2d"；无后缀——既有脚本零改动）──

        /// 物理世界（rapier `PhysicsWorld` 薄壳 + 固定步长积分器；2D）
        #[cfg(feature = "physics2d")]
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame_physics", name = "PhysicsWorld", unhashable = true)]
        pub(crate) struct PyPhysicsWorld {
            pub(crate) inner: Rc<RefCell<SfPhysicsWorld>>,
            /// 碰撞事件通道（step_frame 恒经 collector 步进；drain_events 排空）
            pub(crate) events_rx:
                RefCell<std::sync::mpsc::Receiver<rapier_starfish_debug::dim2::prelude::CollisionEvent>>,
            pub(crate) collector:
                RefCell<rapier_starfish_debug::dim2::prelude::ChannelEventCollector>,
        }

        #[cfg(feature = "physics2d")]
        impl std::fmt::Debug for PyPhysicsWorld {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&format!("PhysicsWorld(<sim_time={}>)", self.inner.borrow().sim_time()))
            }
        }

        /// 刚体 builder（构造 kwargs 收参；insert 一次性消费；2D）
        #[cfg(feature = "physics2d")]
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame_physics", name = "RigidBodyBuilder", unhashable = true)]
        pub(crate) struct PyRigidBodyBuilder {
            pub(crate) builder: RefCell<Option<SfRigidBodyBuilder>>,
        }

        #[cfg(feature = "physics2d")]
        impl std::fmt::Debug for PyRigidBodyBuilder {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("RigidBodyBuilder(<kwargs-built>)")
            }
        }

        /// 碰撞体 builder（构造 kwargs 收参；insert 一次性消费；2D）
        #[cfg(feature = "physics2d")]
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame_physics", name = "ColliderBuilder", unhashable = true)]
        pub(crate) struct PyColliderBuilder {
            pub(crate) builder: RefCell<Option<SfColliderBuilder>>,
        }

        #[cfg(feature = "physics2d")]
        impl std::fmt::Debug for PyColliderBuilder {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("ColliderBuilder(<kwargs-built>)")
            }
        }

        /// 调试渲染控制面（内容位集 = rapier DebugRenderMode；2D）
        #[cfg(feature = "physics2d")]
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame_physics", name = "DebugDraw", unhashable = true)]
        pub(crate) struct PyDebugDraw {
            pub(crate) inner: RefCell<SfDebugDraw>,
        }

        #[cfg(feature = "physics2d")]
        impl std::fmt::Debug for PyDebugDraw {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("DebugDraw(<mode-bits>)")
            }
        }

        /// 线段收集器（collect 的落点；每帧复用零分配；2D）
        #[cfg(feature = "physics2d")]
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame_physics", name = "LineSink", unhashable = true)]
        pub(crate) struct PyLineSink {
            pub(crate) inner: RefCell<SfLineSink>,
        }

        #[cfg(feature = "physics2d")]
        impl std::fmt::Debug for PyLineSink {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&format!("LineSink(<{} lines>)", self.inner.borrow().line_count()))
            }
        }

        /// 调试线绘制器（2D：ortho 相机；GPU 惰性装配——首次 draw 需已
        /// set_mode）
        #[cfg(feature = "physics2d")]
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame_physics", name = "DebugLinesRenderer", unhashable = true)]
        pub(crate) struct PyDebugLinesRenderer {
            pub(crate) ppm: Cell<f32>,
            /// 按样本数分槽（Screen MSAA / 离屏 1x 各持管线——批次 48
            /// 画中画：同一渲染器可同时服务两类目标）
            pub(crate) states:
                RefCell<Vec<(u32, crate::rpy::pygame_physics::runtime_2d::RendererGpu)>>,
        }

        #[cfg(feature = "physics2d")]
        impl std::fmt::Debug for PyDebugLinesRenderer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&format!("DebugLinesRenderer(<ppm={}>)", self.ppm.get()))
            }
        }

        /// collect(world, draw, sink)：物理状态 → 线段流（2D；渲染前每帧
        /// 调用）
        #[cfg(feature = "physics2d")]
        #[pyfunction]
        fn collect(
            world: PyRef<PyPhysicsWorld>,
            draw: PyRef<PyDebugDraw>,
            sink: PyRef<PyLineSink>,
            _vm: &VirtualMachine,
        ) -> PyResult<()> {
            crate::rpy::pygame_physics::runtime_2d::collect(world, draw, sink);
            Ok(())
        }

        // ── 3D 类面（feature "physics3d"；`3D` 后缀与 2D 类共存一模块）──

        /// 物理世界（3D：米制，重力如 gravity=(0, -9.81, 0)）
        #[cfg(feature = "physics3d")]
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame_physics", name = "PhysicsWorld3D", unhashable = true)]
        pub(crate) struct PyPhysicsWorld3D {
            pub(crate) inner: Rc<RefCell<SfPhysicsWorld3>>,
            /// 碰撞事件通道（step_frame 恒经 collector 步进；drain_events 排空）
            pub(crate) events_rx:
                RefCell<std::sync::mpsc::Receiver<rapier_starfish_debug::dim3::prelude::CollisionEvent>>,
            pub(crate) collector:
                RefCell<rapier_starfish_debug::dim3::prelude::ChannelEventCollector>,
        }

        #[cfg(feature = "physics3d")]
        impl std::fmt::Debug for PyPhysicsWorld3D {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&format!("PhysicsWorld3D(<sim_time={}>)", self.inner.borrow().sim_time()))
            }
        }

        /// 刚体 builder（3D：rotation 收欧拉角向量 (rx, ry, rz)）
        #[cfg(feature = "physics3d")]
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame_physics", name = "RigidBodyBuilder3D", unhashable = true)]
        pub(crate) struct PyRigidBodyBuilder3D {
            pub(crate) builder: RefCell<Option<SfRigidBodyBuilder3>>,
        }

        #[cfg(feature = "physics3d")]
        impl std::fmt::Debug for PyRigidBodyBuilder3D {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("RigidBodyBuilder3D(<kwargs-built>)")
            }
        }

        /// 碰撞体 builder（3D：cuboid 收 hx/hy/hz）
        #[cfg(feature = "physics3d")]
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame_physics", name = "ColliderBuilder3D", unhashable = true)]
        pub(crate) struct PyColliderBuilder3D {
            pub(crate) builder: RefCell<Option<SfColliderBuilder3>>,
        }

        #[cfg(feature = "physics3d")]
        impl std::fmt::Debug for PyColliderBuilder3D {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("ColliderBuilder3D(<kwargs-built>)")
            }
        }

        /// 调试渲染控制面（3D）
        #[cfg(feature = "physics3d")]
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame_physics", name = "DebugDraw3D", unhashable = true)]
        pub(crate) struct PyDebugDraw3D {
            pub(crate) inner: RefCell<SfDebugDraw3>,
        }

        #[cfg(feature = "physics3d")]
        impl std::fmt::Debug for PyDebugDraw3D {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("DebugDraw3D(<mode-bits>)")
            }
        }

        /// 线段收集器（3D）
        #[cfg(feature = "physics3d")]
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame_physics", name = "LineSink3D", unhashable = true)]
        pub(crate) struct PyLineSink3D {
            pub(crate) inner: RefCell<SfLineSink3>,
        }

        #[cfg(feature = "physics3d")]
        impl std::fmt::Debug for PyLineSink3D {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&format!("LineSink3D(<{} lines>)", self.inner.borrow().line_count()))
            }
        }

        /// 调试线绘制器（3D：look-at 透视相机 + 深度通道；GPU 惰性装配）
        #[cfg(feature = "physics3d")]
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame_physics", name = "DebugLinesRenderer3D", unhashable = true)]
        pub(crate) struct PyDebugLinesRenderer3D {
            pub(crate) camera: RefCell<crate::rpy::pygame_physics::runtime_3d::Camera3>,
            pub(crate) camera_epoch: Cell<u32>,
            /// 按样本数分槽（同 2D——批次 48 画中画）
            pub(crate) states:
                RefCell<Vec<(u32, crate::rpy::pygame_physics::runtime_3d::RendererGpu3D)>>,
        }

        #[cfg(feature = "physics3d")]
        impl std::fmt::Debug for PyDebugLinesRenderer3D {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let c = self.camera.borrow();
                f.write_str(&format!(
                    "DebugLinesRenderer3D(<eye={:?} fov={:.3}>)",
                    c.eye, c.fov_y
                ))
            }
        }

        /// collect3d(world, draw, sink)：3D 物理状态 → 线段流（渲染前每帧
        /// 调用）
        #[cfg(feature = "physics3d")]
        #[pyfunction]
        fn collect3d(
            world: PyRef<PyPhysicsWorld3D>,
            draw: PyRef<PyDebugDraw3D>,
            sink: PyRef<PyLineSink3D>,
            _vm: &VirtualMachine,
        ) -> PyResult<()> {
            crate::rpy::pygame_physics::runtime_3d::collect(world, draw, sink);
            Ok(())
        }
    }

// ── 文件级 re-export（外部引用归位：pygame 会话的 draw_debug* 钩子）──
#[cfg(feature = "physics2d")]
pub(crate) use pygame_physics::{PyPhysicsWorld, PyRigidBodyBuilder, PyColliderBuilder, PyDebugDraw, PyLineSink, PyDebugLinesRenderer};
#[cfg(any(feature = "physics2d", feature = "physics3d"))]
pub(crate) use pygame_physics::module_def;
#[cfg(feature = "physics3d")]
pub(crate) use pygame_physics::{PyPhysicsWorld3D, PyRigidBodyBuilder3D, PyColliderBuilder3D, PyDebugDraw3D, PyLineSink3D, PyDebugLinesRenderer3D};
