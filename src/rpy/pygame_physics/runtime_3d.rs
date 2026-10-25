//! pygame_physics 3D 类面运行时——rapier-starfish-debug dim3 绑定本体
//! （3D v1，2026-10-24；与 pygame_physics 完全镜像，差异见下）。
//!
//! 维度差异（相对 pygame_physics/2D）：
//! - 相机 = look-at 透视（`set_camera_lookat(eye, target, fov_y)` 弧度；
//!   视口宽高比随屏幕尺寸自动跟随）；
//! - 通道契约**相反**：调试线叠加**必须带深度附件**（3D 线管线
//!   Depth24Plus 硬契约）——`with_pass(..., depth=true)`；
//! - rotation 收**欧拉角向量** (rx, ry, rz)（rapier 0.36 `AngVector`
//!   原生语义，dim3 官例同款）；查询 `rotation()` 返回四元数
//!   (x, y, z, w)（Python 惯例 xyzw 序）；
//! - 坐标契约：米制、y 向上、原点在视口中心（透视相机看向 target）。
//!
//! 帧序/风格定约与 2D 相同（第三方库等价物、不 pygame 化、builder 构造
//! kwargs）——见 pygame_physics/runtime.rs 文档。

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use rapier_starfish_debug::dim3::debug::render::DebugLinesRenderer as SfDebugLinesRenderer;
use rapier_starfish_debug::dim3::debug::{
    collect as sf_collect, DebugDraw as SfDebugDraw, LineSink as SfLineSink,
};
use rapier_starfish_debug::dim3::prelude::{
    ChannelEventCollector, ColliderBuilder as SfColliderBuilder, ColliderHandle,
    CollisionEvent, DebugRenderMode, QueryFilter, Ray as SfRay3,
    ActiveEvents as SfActiveEvents3,
    RigidBodyBuilder as SfRigidBodyBuilder, RigidBodyHandle,
};
use rapier_starfish_debug::dim3::world::PhysicsWorld as SfPhysicsWorld;
use rustpython_derive::{FromArgs, pyclass};
use rustpython_vm::function::ArgIntoFloat;
use rustpython_vm::types::Constructor;
use rustpython_vm::{PyObjectRef, Py, PyRef, PyResult, VirtualMachine, builtins::PyType};

use crate::rpy::args::SfFPoint3;
use super::{
    PyColliderBuilder3D, PyDebugDraw3D, PyDebugLinesRenderer3D, PyLineSink3D, PyPhysicsWorld3D,
    PyRigidBodyBuilder3D,
};

// ── handle 打包（(generation, index) → 不透明 u64；Python 侧当黑盒 int）──

fn pack_body(h: RigidBodyHandle) -> u64 {
    let (index, generation) = h.into_raw_parts();
    ((generation as u64) << 32) | index as u64
}

fn unpack_body(v: u64) -> RigidBodyHandle {
    RigidBodyHandle::from_raw_parts(v as u32, (v >> 32) as u32)
}

fn pack_collider(h: ColliderHandle) -> u64 {
    let (index, generation) = h.into_raw_parts();
    ((generation as u64) << 32) | index as u64
}

fn unpack_collider(v: u64) -> ColliderHandle {
    ColliderHandle::from_raw_parts(v as u32, (v >> 32) as u32)
}

// ── 收参结构体（构造 kwargs）────────────────────────────────────

/// PhysicsWorld(gravity=(0, -9.81, 0), dt=1/60)
#[derive(FromArgs)]
pub(crate) struct PhysicsWorldArgs {
    /// 重力（米/秒²；y 向上为正）
    #[pyarg(any, optional)]
    pub(crate) gravity: Option<SfFPoint3>,
    /// 固定步长（秒；缺省 1/60）
    #[pyarg(any, optional)]
    pub(crate) dt: Option<ArgIntoFloat>,
}

impl Constructor for PyPhysicsWorld3D {
    type Args = PhysicsWorldArgs;

    fn py_new(
        _cls: &Py<PyType>,
        args: Self::Args,
        _vm: &VirtualMachine,
    ) -> PyResult<Self> {
        let g = args.gravity.unwrap_or(SfFPoint3(0.0, -9.81, 0.0));
        let mut world = SfPhysicsWorld::new([g.0, g.1, g.2]);
        if let Some(dt) = args.dt {
            world.set_dt(dt.into_float() as f32);
        }
        // 碰撞事件通道（step_frame 恒经 collector 步进——触发力/撕裂事件
        // 的 sender 无接收端即静默丢弃）
        let (tx, rx) = std::sync::mpsc::channel();
        let (ftx, _frx) = std::sync::mpsc::channel();
        let (_ttx, _trx) = std::sync::mpsc::channel();
        let collector = ChannelEventCollector::new(tx, ftx, _ttx);
        Ok(Self {
            inner: Rc::new(RefCell::new(world)),
            events_rx: RefCell::new(rx),
            collector: RefCell::new(collector),
        })
    }
}

/// RigidBodyBuilder(kind="dynamic"|"fixed"|"kinematic", translation=,
///                  rotation=欧拉角向量, linvel=, angvel=, can_sleep=True)
#[derive(FromArgs)]
pub(crate) struct RigidBodyBuilderArgs {
    #[pyarg(any)]
    pub(crate) kind: String,
    #[pyarg(any, optional)]
    pub(crate) translation: Option<SfFPoint3>,
    /// 欧拉角向量 (rx, ry, rz) 弧度（rapier 0.36 AngVector 原生语义）
    #[pyarg(any, optional)]
    pub(crate) rotation: Option<SfFPoint3>,
    #[pyarg(any, optional)]
    pub(crate) linvel: Option<SfFPoint3>,
    #[pyarg(any, optional)]
    pub(crate) angvel: Option<SfFPoint3>,
    #[pyarg(any, optional)]
    pub(crate) can_sleep: Option<bool>,
}

impl Constructor for PyRigidBodyBuilder3D {
    type Args = RigidBodyBuilderArgs;

    fn py_new(
        _cls: &Py<PyType>,
        args: Self::Args,
        vm: &VirtualMachine,
    ) -> PyResult<Self> {
        let mut b = match args.kind.as_str() {
            "dynamic" => SfRigidBodyBuilder::dynamic(),
            "fixed" => SfRigidBodyBuilder::fixed(),
            "kinematic" => SfRigidBodyBuilder::kinematic_velocity_based(),
            other => {
                return Err(vm.new_value_error(format!(
                    "kind 须为 'dynamic' / 'fixed' / 'kinematic'，实际 {other:?}"
                )))
            }
        };
        if let Some(t) = args.translation {
            b = b.translation([t.0, t.1, t.2].into());
        }
        if let Some(r) = args.rotation {
            b = b.rotation([r.0, r.1, r.2].into());
        }
        if let Some(v) = args.linvel {
            b = b.linvel([v.0, v.1, v.2].into());
        }
        if let Some(w) = args.angvel {
            b = b.angvel([w.0, w.1, w.2].into());
        }
        b = b.can_sleep(args.can_sleep.unwrap_or(true));
        Ok(Self {
            builder: RefCell::new(Some(b)),
        })
    }
}

/// ColliderBuilder(kind="cuboid"|"ball", 半尺寸/半径, friction=,
///                 restitution=, density=, sensor=, translation=)
#[derive(FromArgs)]
pub(crate) struct ColliderBuilderArgs {
    /// "cuboid"（hx/hy/hz 半尺寸）或 "ball"（radius）
    #[pyarg(any)]
    pub(crate) kind: String,
    #[pyarg(any, optional)]
    pub(crate) hx: Option<ArgIntoFloat>,
    #[pyarg(any, optional)]
    pub(crate) hy: Option<ArgIntoFloat>,
    #[pyarg(any, optional)]
    pub(crate) hz: Option<ArgIntoFloat>,
    #[pyarg(any, optional)]
    pub(crate) radius: Option<ArgIntoFloat>,
    /// 相对刚体原点的偏移（米）
    #[pyarg(any, optional)]
    pub(crate) translation: Option<SfFPoint3>,
    #[pyarg(any, optional)]
    pub(crate) friction: Option<ArgIntoFloat>,
    #[pyarg(any, optional)]
    pub(crate) restitution: Option<ArgIntoFloat>,
    #[pyarg(any, optional)]
    pub(crate) density: Option<ArgIntoFloat>,
    /// 传感器（只碰不撞，缺省 false）
    #[pyarg(any, optional)]
    pub(crate) sensor: Option<bool>,
    /// 产生碰撞事件（drain_events 只报开启者——触发器/碰撞检测入口，
    /// 缺省 false 省开销）
    #[pyarg(any, optional)]
    pub(crate) events: Option<bool>,
}

impl Constructor for PyColliderBuilder3D {
    type Args = ColliderBuilderArgs;

    fn py_new(
        _cls: &Py<PyType>,
        args: Self::Args,
        vm: &VirtualMachine,
    ) -> PyResult<Self> {
        let mut c = match args.kind.as_str() {
            "cuboid" => {
                let (Some(hx), Some(hy), Some(hz)) = (args.hx, args.hy, args.hz) else {
                    return Err(vm.new_value_error("cuboid 须给 hx / hy / hz（半尺寸，米）"));
                };
                SfColliderBuilder::cuboid(
                    hx.into_float() as f32,
                    hy.into_float() as f32,
                    hz.into_float() as f32,
                )
            }
            "ball" => {
                let Some(r) = args.radius else {
                    return Err(vm.new_value_error("ball 须给 radius（米）"));
                };
                SfColliderBuilder::ball(r.into_float() as f32)
            }
            other => {
                return Err(vm.new_value_error(format!(
                    "kind 须为 'cuboid' / 'ball'（v1），实际 {other:?}"
                )))
            }
        };
        if let Some(t) = args.translation {
            c = c.translation([t.0, t.1, t.2].into());
        }
        if let Some(f) = args.friction {
            c = c.friction(f.into_float() as f32);
        }
        if let Some(r) = args.restitution {
            c = c.restitution(r.into_float() as f32);
        }
        if let Some(d) = args.density {
            c = c.density(d.into_float() as f32);
        }
        if args.sensor.unwrap_or(false) {
            c = c.sensor(true);
        }
        if args.events.unwrap_or(false) {
            c = c.active_events(SfActiveEvents3::COLLISION_EVENTS);
        }
        Ok(Self {
            builder: RefCell::new(Some(c)),
        })
    }
}

/// DebugDraw(collider_shapes=True, contacts=True, joints=False,
///           aabbs=False, axes=False)——渲染内容位集（rapier 原生语义）
#[derive(FromArgs)]
pub(crate) struct DebugDrawArgs {
    #[pyarg(any, optional)]
    pub(crate) collider_shapes: Option<bool>,
    #[pyarg(any, optional)]
    pub(crate) contacts: Option<bool>,
    #[pyarg(any, optional)]
    pub(crate) joints: Option<bool>,
    #[pyarg(any, optional)]
    pub(crate) aabbs: Option<bool>,
    #[pyarg(any, optional)]
    pub(crate) axes: Option<bool>,
}

fn mode_from_args(args: &DebugDrawArgs, base: DebugRenderMode) -> DebugRenderMode {
    let mut m = base;
    let set = |on: Option<bool>, flag: DebugRenderMode, m: &mut DebugRenderMode| match on {
        Some(true) => *m |= flag,
        Some(false) => *m &= !flag,
        None => {}
    };
    set(args.collider_shapes, DebugRenderMode::COLLIDER_SHAPES, &mut m);
    set(args.contacts, DebugRenderMode::CONTACTS, &mut m);
    set(args.joints, DebugRenderMode::JOINTS, &mut m);
    set(args.aabbs, DebugRenderMode::COLLIDER_AABBS, &mut m);
    set(args.axes, DebugRenderMode::RIGID_BODY_AXES, &mut m);
    m
}

impl Constructor for PyDebugDraw3D {
    type Args = DebugDrawArgs;

    fn py_new(
        _cls: &Py<PyType>,
        args: Self::Args,
        _vm: &VirtualMachine,
    ) -> PyResult<Self> {
        // 缺省 = 原生 DebugDraw::default() 的位集（COLLIDER_SHAPES | CONTACTS）
        let base = DebugRenderMode::COLLIDER_SHAPES | DebugRenderMode::CONTACTS;
        let mode = mode_from_args(&args, base);
        Ok(Self {
            inner: RefCell::new(SfDebugDraw::new(mode)),
        })
    }
}

/// DebugLinesRenderer(eye=(0, 6, 14), target=(0, 0, 0), fov=1.0)——调试线
/// 绘制器（3D：look-at 透视相机；GPU 惰性装配——首次 draw_debug_3d 时
/// 须已 set_mode；fov = 垂直视场角**弧度**）
#[derive(FromArgs)]
pub(crate) struct DebugLinesRendererArgs {
    #[pyarg(any, optional)]
    pub(crate) eye: Option<SfFPoint3>,
    #[pyarg(any, optional)]
    pub(crate) target: Option<SfFPoint3>,
    /// 垂直视场角（**弧度**；1.0 ≈ 57°）
    #[pyarg(any, optional)]
    pub(crate) fov: Option<ArgIntoFloat>,
}

impl Constructor for PyDebugLinesRenderer3D {
    type Args = DebugLinesRendererArgs;

    fn py_new(
        _cls: &Py<PyType>,
        args: Self::Args,
        _vm: &VirtualMachine,
    ) -> PyResult<Self> {
        let eye = args
            .eye
            .unwrap_or(SfFPoint3(0.0, 6.0, 14.0));
        let target = args.target.unwrap_or(SfFPoint3(0.0, 0.0, 0.0));
        Ok(Self {
            camera: RefCell::new(Camera3 {
                eye: [eye.0, eye.1, eye.2],
                target: [target.0, target.1, target.2],
                fov_y: args.fov.map(|f| f.into_float() as f32).unwrap_or(1.0),
            }),
            camera_epoch: Cell::new(1),
            follow_global: Cell::new(false),
            states: RefCell::new(Vec::new()),
        })
    }
}

// ── 相机参数 + 渲染器惰性装配状态（GPU 句柄在首次 draw 时取）──

/// look-at 相机参数（Python 侧可改；draw 时按 epoch/视口脏置应用）
pub(crate) struct Camera3 {
    pub(crate) eye: [f32; 3],
    pub(crate) target: [f32; 3],
    pub(crate) fov_y: f32,
}

pub(crate) struct RendererGpu3D {
    pub(crate) renderer: SfDebugLinesRenderer,
    pub(crate) access: Arc<starfish_render::render_resource_access::RenderResourceAccess>,
    pub(crate) viewport: (u32, u32),
    pub(crate) camera_epoch: u32,
}

// ── pymethod 实现（#[pyclass] impl 块——每类型 crate 内唯一）──────

// builder 无实例方法面——仅声明构造协议归属（RP 空属性 impl 块）

#[pyclass(with(Constructor))]
impl PyRigidBodyBuilder3D {}


#[pyclass(with(Constructor))]
impl PyColliderBuilder3D {}


#[pyclass(with(Constructor))]
impl PyPhysicsWorld3D {
    /// step_frame(real_dt) -> int：真实帧距累积积分（0 = 余量不足一步；
    /// 上限节流防死亡螺旋）
    #[pymethod]
    fn step_frame(&self, real_dt: ArgIntoFloat) -> u32 {
        self.inner
            .borrow_mut()
            .step_frame_with_events(real_dt.into_float() as f32, &*self.collector.borrow())
    }

    /// step()：固定步长积分一次
    #[pymethod]
    fn step(&self) {
        self.inner.borrow_mut().step();
    }

    /// set_dt(dt)：固定步长（秒）
    #[pymethod]
    fn set_dt(&self, dt: ArgIntoFloat) {
        self.inner.borrow_mut().set_dt(dt.into_float() as f32);
    }

    /// sim_time() -> float：已仿真时长（秒；与真实时间解耦）
    #[pymethod]
    fn sim_time(&self) -> f32 {
        self.inner.borrow().sim_time()
    }

    /// insert(body_builder, collider_builder) -> (body_handle, collider_handle)
    /// ——handle 是不透明 int（黑盒，回传给查询方法）；builder 一次性消费
    #[pymethod]
    fn insert(
        &self,
        body: PyRef<PyRigidBodyBuilder3D>,
        collider: PyRef<PyColliderBuilder3D>,
        vm: &VirtualMachine,
    ) -> PyResult<(u64, u64)> {
        let rb = body
            .builder
            .borrow_mut()
            .take()
            .ok_or_else(|| vm.new_value_error("RigidBodyBuilder 已被消费（insert 一次性，重建再插）"))?;
        let cb = collider
            .builder
            .borrow_mut()
            .take()
            .ok_or_else(|| vm.new_value_error("ColliderBuilder 已被消费（insert 一次性，重建再插）"))?;
        let (bh, ch) = self.inner.borrow_mut().get().insert(rb, cb);
        Ok((pack_body(bh), pack_collider(ch)))
    }

    /// translation(body_handle) -> (x, y, z)（米，世界系）
    #[pymethod]
    fn translation(&self, body_handle: u64, vm: &VirtualMachine) -> PyResult<(f32, f32, f32)> {
        let w = self.inner.borrow();
        let body = w
            .get_ref()
            .bodies
            .get(unpack_body(body_handle))
            .ok_or_else(|| vm.new_value_error("无效的 body handle"))?;
        let t = body.translation();
        Ok((t.x, t.y, t.z))
    }

    /// rotation(body_handle) -> (x, y, z, w)：姿态四元数（Python 惯例
    /// xyzw 序；nalgebra 内部标量在前的换序在此收口）
    #[pymethod]
    fn rotation(&self, body_handle: u64, vm: &VirtualMachine) -> PyResult<(f32, f32, f32, f32)> {
        let w = self.inner.borrow();
        let body = w
            .get_ref()
            .bodies
            .get(unpack_body(body_handle))
            .ok_or_else(|| vm.new_value_error("无效的 body handle"))?;
        let q = body.rotation();
        Ok((q.x, q.y, q.z, q.w))
    }

    /// linvel(body_handle) -> (vx, vy, vz)（米/秒）
    #[pymethod]
    fn linvel(&self, body_handle: u64, vm: &VirtualMachine) -> PyResult<(f32, f32, f32)> {
        let w = self.inner.borrow();
        let body = w
            .get_ref()
            .bodies
            .get(unpack_body(body_handle))
            .ok_or_else(|| vm.new_value_error("无效的 body handle"))?;
        let v = body.linvel();
        Ok((v.x, v.y, v.z))
    }

    /// is_sleeping(body_handle) -> bool（静止体休眠态）
    #[pymethod]
    fn is_sleeping(&self, body_handle: u64, vm: &VirtualMachine) -> PyResult<bool> {
        let w = self.inner.borrow();
        let body = w
            .get_ref()
            .bodies
            .get(unpack_body(body_handle))
            .ok_or_else(|| vm.new_value_error("无效的 body handle"))?;
        Ok(body.is_sleeping())
    }

    // ── 写入面（3D v2：游戏操控 / 触发器 / 查询——与 2D 同款语义）──

    /// set_translation(body_handle, (x, y, z))：瞬移（自动唤醒）
    #[pymethod]
    fn set_translation(
        &self,
        body_handle: u64,
        pos: SfFPoint3,
        vm: &VirtualMachine,
    ) -> PyResult<()> {
        let mut w = self.inner.borrow_mut();
        let body = w
            .get()
            .bodies
            .get_mut(unpack_body(body_handle))
            .ok_or_else(|| vm.new_value_error("无效的 body handle"))?;
        body.set_translation([pos.0, pos.1, pos.2].into(), true);
        Ok(())
    }

    /// set_rotation(body_handle, (rx, ry, rz))：设姿态（欧拉角向量弧度；
    /// 自动唤醒——与构造 kwargs 同语义）
    #[pymethod]
    fn set_rotation(
        &self,
        body_handle: u64,
        r: SfFPoint3,
        vm: &VirtualMachine,
    ) -> PyResult<()> {
        let mut w = self.inner.borrow_mut();
        let body = w
            .get()
            .bodies
            .get_mut(unpack_body(body_handle))
            .ok_or_else(|| vm.new_value_error("无效的 body handle"))?;
        body.set_rotation(
            rapier_starfish_debug::dim3::prelude::rotation_from_angle([r.0, r.1, r.2].into()),
            true,
        );
        Ok(())
    }

    /// set_linvel(body_handle, (vx, vy, vz))：设线速度（自动唤醒）
    #[pymethod]
    fn set_linvel(&self, body_handle: u64, v: SfFPoint3, vm: &VirtualMachine) -> PyResult<()> {
        let mut w = self.inner.borrow_mut();
        let body = w
            .get()
            .bodies
            .get_mut(unpack_body(body_handle))
            .ok_or_else(|| vm.new_value_error("无效的 body handle"))?;
        body.set_linvel([v.0, v.1, v.2].into(), true);
        Ok(())
    }

    /// set_angvel(body_handle, (wx, wy, wz))：设角速度（自动唤醒）
    #[pymethod]
    fn set_angvel(&self, body_handle: u64, w: SfFPoint3, vm: &VirtualMachine) -> PyResult<()> {
        let mut world = self.inner.borrow_mut();
        let body = world
            .get()
            .bodies
            .get_mut(unpack_body(body_handle))
            .ok_or_else(|| vm.new_value_error("无效的 body handle"))?;
        body.set_angvel([w.0, w.1, w.2].into(), true);
        Ok(())
    }

    /// apply_impulse(body_handle, (ix, iy, iz))：施加冲量（自动唤醒）
    #[pymethod]
    fn apply_impulse(
        &self,
        body_handle: u64,
        impulse: SfFPoint3,
        vm: &VirtualMachine,
    ) -> PyResult<()> {
        let mut w = self.inner.borrow_mut();
        let body = w
            .get()
            .bodies
            .get_mut(unpack_body(body_handle))
            .ok_or_else(|| vm.new_value_error("无效的 body handle"))?;
        body.apply_impulse([impulse.0, impulse.1, impulse.2].into(), true);
        Ok(())
    }

    /// wake_up(body_handle)：唤醒休眠体
    #[pymethod]
    fn wake_up(&self, body_handle: u64, vm: &VirtualMachine) -> PyResult<()> {
        let mut w = self.inner.borrow_mut();
        w.get().wake_up(unpack_body(body_handle), false);
        Ok(())
    }

    /// set_gravity((gx, gy, gz))：改重力（下一帧生效）
    #[pymethod]
    fn set_gravity(&self, g: SfFPoint3) {
        self.inner.borrow_mut().get().gravity = [g.0, g.1, g.2].into();
    }

    /// remove_body(body_handle) -> bool：移除刚体（连带其全部碰撞体；
    /// 返回是否存在并已移除）
    #[pymethod]
    fn remove_body(&self, body_handle: u64, vm: &VirtualMachine) -> PyResult<bool> {
        let mut w = self.inner.borrow_mut();
        Ok(w.get()
            .remove_body_with_colliders(unpack_body(body_handle), true)
            .is_some())
    }

    /// drain_events() -> [(kind, collider_handle1, collider_handle2), ...]：
    /// 排空自上帧以来的碰撞事件（kind = "started" | "stopped"；仅
    /// `ColliderBuilder3D(events=True)` 的碰撞体产生事件——传感器触发器
    /// 同此通道）。每帧调用一次，不清也不丢（通道缓冲按步累积）。
    #[pymethod]
    fn drain_events(&self, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
        let mut out: Vec<PyObjectRef> = Vec::new();
        loop {
            let ev = match self.events_rx.borrow_mut().try_recv() {
                Ok(ev) => ev,
                Err(_) => break,
            };
            let (kind, h1, h2) = match ev {
                CollisionEvent::Started(h1, h2, _) => ("started", h1, h2),
                CollisionEvent::Stopped(h1, h2, _) => ("stopped", h1, h2),
                _ => continue,
            };
            let s: PyObjectRef = vm.ctx.new_str(kind).into();
            let a: PyObjectRef = vm.ctx.new_int(pack_collider(h1) as i64).into();
            let b: PyObjectRef = vm.ctx.new_int(pack_collider(h2) as i64).into();
            let tup: PyObjectRef = vm.new_tuple((s, a, b)).into();
            out.push(tup);
        }
        Ok(vm.ctx.new_list(out).into())
    }
    /// 射线查询（dir 非单位向量按其模长定射线长；max_toi 为 dir 模长的
    /// 倍数上限，None = 缺省 100.0——RP 方法参数无缺省，Python 侧显式传
    /// None）。⚠️ 时序契约：insert 后**须至少 step 一次**方可查询
    /// （broad_phase 时序——实测 insert 后立即查询返回 None）。
    /// ⚠️ 时序契约：insert 后**须至少 step 一次**方可查询（broad_phase
    /// 时序——实测 insert 后立即查询返回 None）。
    #[pymethod]
    fn cast_ray(
        &self,
        origin: SfFPoint3,
        dir: SfFPoint3,
        max_toi: Option<ArgIntoFloat>,
        vm: &VirtualMachine,
    ) -> PyResult<Option<(u64, f64)>> {
        let w = self.inner.borrow();
        let ray = SfRay3::new(
            [origin.0, origin.1, origin.2].into(),
            [dir.0, dir.1, dir.2].into(),
        );
        let max = max_toi.map(|m| m.into_float() as f32).unwrap_or(100.0);
        Ok(w.get_ref()
            .cast_ray(&ray, max, true, QueryFilter::default())
            .map(|(ch, toi)| (pack_collider(ch), toi as f64)))
    }

    /// intersection_pair(h1, h2) -> bool：两碰撞体当前是否相交
    #[pymethod]
    fn intersection_pair(&self, h1: u64, h2: u64, vm: &VirtualMachine) -> PyResult<bool> {
        let w = self.inner.borrow();
        Ok(w.get_ref()
            .intersection_pair(unpack_collider(h1), unpack_collider(h2))
            .is_some())
    }
}


#[pyclass(with(Constructor))]
impl PyLineSink3D {
    /// line_count() -> int：当前线段数（collect 后有效）
    #[pymethod]
    fn line_count(&self) -> u64 {
        self.inner.borrow().line_count() as u64
    }

    /// is_empty() -> bool
    #[pymethod]
    fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }

    /// clear()：清空缓冲（保 capacity）
    #[pymethod]
    fn clear(&self) {
        self.inner.borrow_mut().clear();
    }
}

impl Constructor for PyLineSink3D {
    type Args = ();

    fn py_new(
        _cls: &Py<PyType>,
        args: Self::Args,
        _vm: &VirtualMachine,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: RefCell::new(SfLineSink::new()),
        })
    }
}

// 调试渲染控制面无实例方法面（内容位集构造期定格）——空 impl 块归属

#[pyclass(with(Constructor))]
impl PyDebugDraw3D {}


#[pyclass(with(Constructor))]
impl PyDebugLinesRenderer3D {
    /// set_camera_lookat(eye, target, fov)：look-at 透视相机（fov =
    /// 垂直视场角**弧度**；下次 draw_debug_3d 生效，视口宽高比自动跟随）
    #[pymethod]
    fn set_camera_lookat(&self, eye: SfFPoint3, target: SfFPoint3, fov: ArgIntoFloat) {
        let mut c = self.camera.borrow_mut();
        c.eye = [eye.0, eye.1, eye.2];
        c.target = [target.0, target.1, target.2];
        c.fov_y = fov.into_float() as f32;
        drop(c);
        self.camera_epoch.set(self.camera_epoch.get() + 1);
        self.follow_global.set(false); // 显式相机优先（全局跟随解除）
    }

    /// follow_global_camera(enable=True)：切全局相机跟随（渲染架构
    /// 批次 B）——draw 前同步引擎全局 MVP（`pygame.render.set_camera`
    /// 一处设置、全体生效；Auto 状态按目标尺寸自动解算）。再调
    /// set_camera_lookat 即解除。
    #[pymethod]
    fn follow_global_camera(&self, enable: Option<bool>) {
        self.follow_global.set(enable.unwrap_or(true));
    }
}

// ── 模块级自由函数（mod.rs 的 #[pymodule] 面委托目标）──────────────

/// collect(world, draw, sink)：物理状态 → 线段流（每帧渲染前调用；
/// sink 内部先清后收，零分配复用）
pub(crate) fn collect(
    world: PyRef<PyPhysicsWorld3D>,
    draw: PyRef<PyDebugDraw3D>,
    sink: PyRef<PyLineSink3D>,
) {
    sf_collect(
        &world.inner.borrow(),
        &mut draw.inner.borrow_mut(),
        &mut sink.inner.borrow_mut(),
    );
}

/// draw_debug_3d(renderer, sink)（`t.draw_debug_3d(renderer, sink)` 的
/// 本体，t = `with screen.render()` 的会话对象）：GPU 惰性装配 + look-at
/// 相机跟随（调用时点尺寸快照）→ 经活跃会话排程延迟叠加（引擎泛型
/// `RenderPass::defer_overlay`——零 rapier 感知）。**3D 通道带深度附件**
/// （with_pass depth=true——3D 线管线硬契约，与 2D 相反）；深度附件每
/// 上下文一条，本通道执行于游戏会话全部 pass 收口之后（deferred），独占
/// 无冲突。每帧一次。
pub(crate) fn draw_debug_3d(
    renderer: PyRef<PyDebugLinesRenderer3D>,
    sink: PyRef<PyLineSink3D>,
    vm: &VirtualMachine,
) -> PyResult<()> {
    // 会话探针先行（同 draw_ui/draw_debug——会话外调用先报错）
    starfish_pygame::render::session::with_screen(|_| {}).map_err(|_| {
        vm.new_type_error(
            "draw_debug_3d 须在 `with screen.render():` 会话块内调用（t = 会话对象）",
        )
    })?;
    let screen = starfish_pygame::display::get_screen();
    let samples = screen.sample_count();
    let size = starfish_pygame::render::DrawTarget::size(screen);
    let gpu = starfish_pygame::render::gpu_info().ok_or_else(|| {
        vm.new_runtime_error("GPU 未就绪：须在 pygame.display.set_mode 之后调用")
    })?;
    draw_debug_3d_generic(renderer, sink, samples, size, false, &gpu, |boxed| {
        starfish_pygame::render::session::with_screen(|pass| {
            pass.defer_overlay(boxed);
        })
        .expect("会话探针刚确认存在（同栈同步，无中途收口路径）");
    })
}

/// Surface 变体（`t.draw_debug_3d(renderer, sink)`，t = `with surf.render()`
/// 会话对象）——排程到本面 flush 收口（画中画物理动画）。样本数 = 1
/// （离屏面无 MSAA），**表面须 `depth_buffer=True` 创建**（3D 线管线
/// 深度通道硬契约——无深度面 wgpu 校验 panic，错误信息已指路）。
pub(crate) fn draw_debug_3d_on_surface(
    renderer: PyRef<PyDebugLinesRenderer3D>,
    sink: PyRef<PyLineSink3D>,
    surface: &starfish_pygame::render::Surface,
    vm: &VirtualMachine,
) -> PyResult<()> {
    let gpu = starfish_pygame::render::gpu_info().ok_or_else(|| {
        vm.new_runtime_error("GPU 未就绪：须在 pygame.display.set_mode 之后调用")
    })?;
    draw_debug_3d_generic(
        renderer,
        sink,
        1, // 离屏：无 MSAA
        starfish_pygame::render::DrawTarget::size(surface),
        true, // 离屏目标：管线按 Rgba8Unorm 离屏 access 建
        &gpu,
        |boxed| surface.defer_overlay(boxed),
    )
}

/// draw_debug_3d 本体（调度泛化，批次 48）：GPU 惰性装配（**按
/// (样本数, 目标系) 分槽**——管线按 (color_format, sample_count) 分套
/// 是 wgpu 硬约束：Screen = 交换链格式 access / Surface = Rgba8Unorm
/// 离屏 access，交换链格式随平台漂移不得跨目标复用）+ look-at 相机
/// 跟随（viewport/epoch 脏置）→ 经 schedule 排程（Screen = present
/// 收口 / Surface = flush 收口）。深度通道契约同前。
fn draw_debug_3d_generic(
    renderer: PyRef<PyDebugLinesRenderer3D>,
    sink: PyRef<PyLineSink3D>,
    samples: u32,
    size: (u32, u32),
    offscreen: bool,
    gpu: &starfish_pygame::render::GpuInfo,
    schedule: impl FnOnce(Box<dyn FnOnce(&mut starfish_pygame::render::OverlayCtx) + 'static>),
) -> PyResult<()> {
    // 按目标系选 access：离屏 = Rgba8Unorm（Surface 恒此格式）/ 主屏 =
    // 交换链格式（Web/macOS Bgra8、Windows Rgba8——按 access 默认值建）
    let access = if offscreen {
        gpu.offscreen_access.clone()
    } else {
        gpu.access.clone()
    };
    {
        let mut slots = renderer.states.borrow_mut();
        let slot = match slots.iter_mut().find(|(k, _)| (k.0, k.1) == (samples, offscreen)) {
            Some(slot) => slot,
            None => {
                slots.push((
                    (samples, offscreen),
                    RendererGpu3D {
                        renderer: SfDebugLinesRenderer::new(&access, samples),
                        access,
                        viewport: (0, 0),
                        camera_epoch: 0,
                    },
                ));
                slots.last_mut().expect("槽刚 push")
            }
        };
        let st = &mut slot.1;
        let camera = renderer.camera.borrow();
        if renderer.follow_global.get() {
            // 全局相机跟随（批次 B）：每帧同步引擎全局 MVP（Manual 用户
            // override / Auto 按目标尺寸——set_camera 一处设置全体生效）
            st.renderer.set_camera_mvp(
                &st.access,
                starfish_pygame::render::camera_mvp_for(size).to_cols_array_2d(),
            );
            st.viewport = size;
            st.camera_epoch = renderer.camera_epoch.get();
        } else if st.viewport != size || st.camera_epoch != renderer.camera_epoch.get() {
            st.renderer.set_camera_lookat(
                &st.access,
                camera.eye,
                camera.target,
                camera.fov_y,
                size,
            );
            st.viewport = size;
            st.camera_epoch = renderer.camera_epoch.get();
        }
    }
    // 排程（PyRef 'static——闭包持引用计数，present/flush 时段解引用载荷；
    // 主线程单线程契约下无并发借用）
    let key = (samples, offscreen);
    schedule(Box::new(move |octx| {
        let mut slots = renderer.states.borrow_mut();
        if let Some(slot) = slots.iter_mut().find(|(k, _)| *k == key) {
            let sink_ref = sink.inner.borrow();
            // 3D 通道契约：**带深度附件**（与 2D 的 false 相反）
            octx.with_pass("rapier_debug_3d", true, |pass| {
                slot.1.renderer.draw(&slot.1.access, pass, &sink_ref);
            });
        }
    }));
    Ok(())
}
