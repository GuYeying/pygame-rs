//! pygame.rapier —— rapier-starfish-debug 原生风格绑定面（2D v1，2026-10-24）。
//!
//! 接口风格定约：**第三方库等价物，不 pygame 化**——类型/方法名与
//! rapier-starfish-debug（rapier 0.36 词汇）一致，Rust 侧文档直接适用；
//! Python 难度控制 = builder 配置全走构造 kwargs（RP 链式返回 self 的
//! 成本 > 收益，v1 不做链式——原生 `RigidBodyBuilder::dynamic().translation(..)`
//! 的链式位在这里是 `RigidBodyBuilder("dynamic", translation=..)`）。
//!
//! 帧序定约（对齐 rapier-starfish-debug 设计文档 §6）：
//! `event 泵 → step_frame(dt) → collect → 游戏会话绘制 →
//! renderer.draw(sink)（叠加通道）→ flip`。**叠加绘制必须在 render 会话
//! 之后、flip 之前**（提交序 = 画家序；放会话内会被游戏 batch 覆盖）。
//!
//! 坐标契约：物理世界 = 米制、y 向上、原点在视口中心（ortho 相机，
//! `set_camera(ppm)` 调缩放）——与 pygame 像素坐标（y 向下、左上原点）
//! **不同系**，调试线独立成景显示。
//!
//! 惯例提醒（rapier 行为，非 bug）：默认 `sleep_color_multiplier` 把静止
//! 体画暗（亮度 ×0.2）——"物体不动就变暗消失"是 rapier 状态编码。
//!
//! 结构体声明 + `#[pyclass] impl` 块在 `mods/rapier.rs` 的 pymodule
//! 树内（RP 宏要求 struct 与方法收集块同宿；批次 43 拆分定约），
//! 本文件 = 会话实现本体（pyegui 同构：运行时 + draw_debug 排程）。

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use rapier_starfish_debug::dim2::debug::render::DebugLinesRenderer as SfDebugLinesRenderer;
use rapier_starfish_debug::dim2::debug::{collect as sf_collect, DebugDraw as SfDebugDraw,
    LineSink as SfLineSink};
use rapier_starfish_debug::dim2::prelude::{
    ColliderBuilder as SfColliderBuilder, ColliderHandle, DebugRenderMode,
    RigidBodyBuilder as SfRigidBodyBuilder, RigidBodyHandle,
};
use rapier_starfish_debug::dim2::world::PhysicsWorld as SfPhysicsWorld;
use rustpython_derive::{FromArgs, pyclass};
use rustpython_vm::function::ArgIntoFloat;
use rustpython_vm::types::Constructor;
use rustpython_vm::{Py, PyRef, PyResult, VirtualMachine, builtins::PyType};

use crate::rpy::args::SfFPoint;
use crate::rpy::mods::rapier::{
    PyColliderBuilder, PyDebugDraw, PyDebugLinesRenderer, PyLineSink, PyPhysicsWorld,
    PyRigidBodyBuilder,
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

// ── 收参结构体（构造 kwargs）────────────────────────────────────

/// PhysicsWorld(gravity=(0, -9.81), dt=1/60)
#[derive(FromArgs)]
pub(crate) struct PhysicsWorldArgs {
    /// 重力（米/秒²；y 向上为正）
    #[pyarg(any, optional)]
    pub(crate) gravity: Option<SfFPoint>,
    /// 固定步长（秒；缺省 1/60）
    #[pyarg(any, optional)]
    pub(crate) dt: Option<ArgIntoFloat>,
}

impl Constructor for PyPhysicsWorld {
    type Args = PhysicsWorldArgs;

    fn py_new(
        _cls: &Py<PyType>,
        args: Self::Args,
        _vm: &VirtualMachine,
    ) -> PyResult<Self> {
        let g = args.gravity.unwrap_or(SfFPoint(0.0, -9.81));
        let mut world = SfPhysicsWorld::new([g.0, g.1]);
        if let Some(dt) = args.dt {
            world.set_dt(dt.into_float() as f32);
        }
        Ok(Self {
            inner: Rc::new(RefCell::new(world)),
        })
    }
}

/// RigidBodyBuilder(kind="dynamic"|"fixed"|"kinematic", translation=,
///                  rotation=弧度, linvel=, angvel=, can_sleep=True)
#[derive(FromArgs)]
pub(crate) struct RigidBodyBuilderArgs {
    #[pyarg(any)]
    pub(crate) kind: String,
    #[pyarg(any, optional)]
    pub(crate) translation: Option<SfFPoint>,
    /// 弧度（rapier 原生单位）
    #[pyarg(any, optional)]
    pub(crate) rotation: Option<ArgIntoFloat>,
    #[pyarg(any, optional)]
    pub(crate) linvel: Option<SfFPoint>,
    #[pyarg(any, optional)]
    pub(crate) angvel: Option<ArgIntoFloat>,
    #[pyarg(any, optional)]
    pub(crate) can_sleep: Option<bool>,
}

impl Constructor for PyRigidBodyBuilder {
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
            b = b.translation([t.0, t.1].into());
        }
        if let Some(r) = args.rotation {
            b = b.rotation(r.into_float() as f32);
        }
        if let Some(v) = args.linvel {
            b = b.linvel([v.0, v.1].into());
        }
        if let Some(w) = args.angvel {
            b = b.angvel(w.into_float() as f32);
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
    /// "cuboid"（hx/hy 半尺寸）或 "ball"（radius）
    #[pyarg(any)]
    pub(crate) kind: String,
    #[pyarg(any, optional)]
    pub(crate) hx: Option<ArgIntoFloat>,
    #[pyarg(any, optional)]
    pub(crate) hy: Option<ArgIntoFloat>,
    #[pyarg(any, optional)]
    pub(crate) radius: Option<ArgIntoFloat>,
    /// 相对刚体原点的偏移（米）
    #[pyarg(any, optional)]
    pub(crate) translation: Option<SfFPoint>,
    #[pyarg(any, optional)]
    pub(crate) friction: Option<ArgIntoFloat>,
    #[pyarg(any, optional)]
    pub(crate) restitution: Option<ArgIntoFloat>,
    #[pyarg(any, optional)]
    pub(crate) density: Option<ArgIntoFloat>,
    /// 传感器（只碰不撞，缺省 false）
    #[pyarg(any, optional)]
    pub(crate) sensor: Option<bool>,
}

impl Constructor for PyColliderBuilder {
    type Args = ColliderBuilderArgs;

    fn py_new(
        _cls: &Py<PyType>,
        args: Self::Args,
        vm: &VirtualMachine,
    ) -> PyResult<Self> {
        let mut c = match args.kind.as_str() {
            "cuboid" => {
                let (Some(hx), Some(hy)) = (args.hx, args.hy) else {
                    return Err(vm.new_value_error("cuboid 须给 hx / hy（半尺寸，米）"));
                };
                SfColliderBuilder::cuboid(hx.into_float() as f32, hy.into_float() as f32)
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
            c = c.translation([t.0, t.1].into());
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

fn mode_from_args(
    args: &DebugDrawArgs,
    base: DebugRenderMode,
) -> DebugRenderMode {
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

impl Constructor for PyDebugDraw {
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

/// DebugLinesRenderer(ppm=50.0)——调试线绘制器（GPU 惰性装配：首次
/// draw() 时须已 set_mode；表面尺寸变化自动重设相机）
#[derive(FromArgs)]
pub(crate) struct DebugLinesRendererArgs {
    /// 像素/米（ortho 相机缩放）
    #[pyarg(any, optional)]
    pub(crate) ppm: Option<ArgIntoFloat>,
}

impl Constructor for PyDebugLinesRenderer {
    type Args = DebugLinesRendererArgs;

    fn py_new(
        _cls: &Py<PyType>,
        args: Self::Args,
        _vm: &VirtualMachine,
    ) -> PyResult<Self> {
        Ok(Self {
            ppm: Cell::new(args.ppm.map(|p| p.into_float() as f32).unwrap_or(50.0)),
            state: RefCell::new(None),
        })
    }
}

// ── 渲染器惰性装配状态（GPU 句柄在首次 draw 时取——set_mode 后必有）──

pub(crate) struct RendererGpu {
    pub(crate) renderer: SfDebugLinesRenderer,
    pub(crate) access: Arc<starfish_render::render_resource_access::RenderResourceAccess>,
    pub(crate) viewport: (u32, u32),
}

// ── pymethod 实现（#[pyclass] impl 块——每类型 crate 内唯一）──────

// builder 无实例方法面——仅声明构造协议归属（RP 空属性 impl 块）
#[pyclass(with(Constructor))]
impl PyRigidBodyBuilder {}

#[pyclass(with(Constructor))]
impl PyColliderBuilder {}

#[pyclass(with(Constructor))]
impl PyPhysicsWorld {
    /// step_frame(real_dt) -> int：真实帧距累积积分（0 = 余量不足一步；
    /// 上限节流防死亡螺旋）
    #[pymethod]
    fn step_frame(&self, real_dt: ArgIntoFloat) -> u32 {
        self.inner.borrow_mut().step_frame(real_dt.into_float() as f32)
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
        body: PyRef<PyRigidBodyBuilder>,
        collider: PyRef<PyColliderBuilder>,
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

    /// translation(body_handle) -> (x, y)（米，世界系）
    #[pymethod]
    fn translation(&self, body_handle: u64, vm: &VirtualMachine) -> PyResult<(f32, f32)> {
        let w = self.inner.borrow();
        let body = w
            .get_ref()
            .bodies
            .get(unpack_body(body_handle))
            .ok_or_else(|| vm.new_value_error("无效的 body handle"))?;
        let t = body.translation();
        Ok((t.x, t.y))
    }

    /// rotation(body_handle) -> float（弧度）
    #[pymethod]
    fn rotation(&self, body_handle: u64, vm: &VirtualMachine) -> PyResult<f32> {
        let w = self.inner.borrow();
        let body = w
            .get_ref()
            .bodies
            .get(unpack_body(body_handle))
            .ok_or_else(|| vm.new_value_error("无效的 body handle"))?;
        Ok(body.rotation().angle())
    }

    /// linvel(body_handle) -> (vx, vy)（米/秒）
    #[pymethod]
    fn linvel(&self, body_handle: u64, vm: &VirtualMachine) -> PyResult<(f32, f32)> {
        let w = self.inner.borrow();
        let body = w
            .get_ref()
            .bodies
            .get(unpack_body(body_handle))
            .ok_or_else(|| vm.new_value_error("无效的 body handle"))?;
        let v = body.linvel();
        Ok((v.x, v.y))
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
}

#[pyclass(with(Constructor))]
impl PyDebugDraw {
    /// set_mode(collider_shapes=, contacts=, joints=, aabbs=, axes=)
    /// ——运行期改渲染内容位（只更新给出的位）
    #[pymethod]
    fn set_mode(&self, args: DebugDrawArgs) {
        let mut d = self.inner.borrow_mut();
        let base = d.pipeline().mode;
        d.pipeline().mode = mode_from_args(&args, base);
    }
}

#[pyclass(with(Constructor))]
impl PyLineSink {
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

impl Constructor for PyLineSink {
    type Args = ();

    fn py_new(
        _cls: &Py<PyType>,
        _args: Self::Args,
        _vm: &VirtualMachine,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: RefCell::new(SfLineSink::new()),
        })
    }
}

#[pyclass(with(Constructor))]
impl PyDebugLinesRenderer {
    /// set_camera(ppm)：ortho 缩放（像素/米；下次 draw_debug 生效）
    #[pymethod]
    fn set_camera(&self, ppm: ArgIntoFloat) {
        self.ppm.set(ppm.into_float() as f32);
        if let Some(st) = self.state.borrow_mut().as_mut() {
            st.viewport = (0, 0); // 置脏 → 下次 draw_debug 重设相机
        }
    }
}

// ── 模块级自由函数（mods/rapier.rs 的 #[pymodule] 面委托目标）──────

/// collect(world, draw, sink)：物理状态 → 线段流（每帧渲染前调用；
/// sink 内部先清后收，零分配复用）
pub(crate) fn collect(
    world: PyRef<PyPhysicsWorld>,
    draw: PyRef<PyDebugDraw>,
    sink: PyRef<PyLineSink>,
) {
    sf_collect(
        &world.inner.borrow(),
        &mut draw.inner.borrow_mut(),
        &mut sink.inner.borrow_mut(),
    );
}

/// draw_debug(renderer, sink)（`t.draw_debug(renderer, sink)` 的本体，
/// t = `with screen.render()` 的会话对象）：GPU 惰性装配 + 相机跟随
/// （调用时点）→ 经活跃会话排程延迟叠加（引擎泛型
/// `RenderPass::defer_overlay`——零 rapier 感知）。与 egui 的
/// `t.draw_ui(panel)` 同一渲染行为：会话内调用、恒在游戏内容之上、
/// flip 时单次落屏。每帧一次；2D 通道无深度附件（线管线硬契约）。
pub(crate) fn draw_debug(
    renderer: PyRef<PyDebugLinesRenderer>,
    sink: PyRef<PyLineSink>,
    vm: &VirtualMachine,
) -> PyResult<()> {
    // 会话探针先行（同 draw_ui——会话外调用先报错）
    starfish_pygame::render::session::with_screen(|_| {}).map_err(|_| {
        vm.new_type_error("draw_debug 须在 `with screen.render():` 会话块内调用（t = 会话对象）")
    })?;
    // GPU 惰性装配 + 相机跟随（调用时点尺寸快照；帧中 resize 由引擎
    // 帧边界应用，下一帧相机置脏重设）
    {
        let gpu = starfish_pygame::render::gpu_info().ok_or_else(|| {
            vm.new_runtime_error("GPU 未就绪：须在 pygame.display.set_mode 之后调用")
        })?;
        let screen = starfish_pygame::display::get_screen();
        let size = starfish_pygame::render::DrawTarget::size(screen);
        let mut st = renderer.state.borrow_mut();
        if st.is_none() {
            // 表面就位后装配（管线格式取 access 默认表面格式 + MSAA 匹配）
            *st = Some(RendererGpu {
                renderer: SfDebugLinesRenderer::new(&gpu.access, screen.sample_count()),
                access: gpu.access.clone(),
                viewport: (0, 0),
            });
        }
        let st = st.as_mut().expect("state 刚写入");
        if st.viewport != size {
            st.renderer
                .set_camera_ortho(&st.access, size, renderer.ppm.get());
            st.viewport = size;
        }
    }
    // 排程（PyRef 'static——闭包持引用计数，present 时段解引用载荷；
    // 主线程单线程契约下无并发借用）
    starfish_pygame::render::session::with_screen(|pass| {
        pass.defer_overlay(move |octx| {
            let mut st = renderer.state.borrow_mut();
            if let Some(g) = st.as_mut() {
                let sink_ref = sink.inner.borrow();
                octx.with_pass("rapier_debug", false, |pass| {
                    g.renderer.draw(&g.access, pass, &sink_ref);
                });
            }
        });
    })
    .expect("会话探针刚确认存在（同栈同步，无中途收口路径）");
    Ok(())
}
