//! 引擎桥：宿主壳（帧循环）与 Python 面（native_pygame）共享的
//! 引擎状态 + 会话/事件/绘制真身。
//!
//! 职责（为什么这个文件独立存在——它不是调用层，是**状态归属**）：
//! - **thread-local 状态**：`SESSION`（当前 RenderPass）、`EVENTS` 队列、
//!   `DIAG` 诊断计数——宿主壳在解释器外每帧调 [`pump`]，Python 调用
//!   （原生 pygame 模块）在解释器内读写同一份状态；
//! - **宿主 API**：[`pump`] / [`drain_events`] / [`devlog`]；
//! - **绘制真身**：`sf_draw_*` 路由到活跃会话（RenderPass 实现
//!   DrawTarget），收参结构体在 [`crate::rpy::args`]；
//! - **注册面**（临时）：`sf_draw_*` 全局 = **签名契约探针面**（sig_probe
//!   锚定；正式入口在原生 pygame 模块）。旧扁平别名已随 rp_main_loop /
//!   window_test 退役移除（存档 examples/legacy/）。
//!
//! 全部走 starfish 公共 API：零 RP 类型外泄、零 starfish 反向感知。

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use rustpython_vm::builtins::PyBaseExceptionRef;
use rustpython_vm::convert::TryFromObject;
use rustpython_vm::scope::Scope;
use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine};

use crate::rpy::args::{
    BlitAreaArgs, DrawArcArgs, DrawCircleArgs, DrawEllipseArgs, DrawLineArgs, DrawPolygonArgs,
    DrawRectArgs, FillArgs, QuadCornersArgs, SfColor, SfPoint, SfRect, GeometryArgs,
    VerticesArgs,
};

use starfish::base::audio::{AudioMixer, SoundData};
use starfish::base::gfx::{Geometry, ShapeVertex};
use starfish::base::window::KeyCode;
use starfish::pygame::color::Color;
use starfish::pygame::rect::Rect;
use starfish::pygame::render::{BatchTopology, GeometryKind, SpriteVertex};
use starfish::pygame::render::RenderPass;
use starfish::pygame::{display, draw, time};
use starfish::pygame::Event;

thread_local! {
    /// 待派发事件（pump 灌入，drain_events 排空——pygame.event.get 的货源）
    static EVENTS: RefCell<VecDeque<Event>> = RefCell::new(VecDeque::new());
    /// ADR-5 渲染会话（`with screen.render():` 的承载；get_screen() 是
    /// &'static，故 RenderPass<'static> 可驻留 thread-local）
    static SESSION: RefCell<Option<RenderPass<'static>>> = const { RefCell::new(None) };
    /// 诊断计数器（sf_draw_rect 首调门）
    static DIAG: DiagCount = DiagCount::new();
}

/// 诊断计数（真机排障：调用次数日志化）
struct DiagCount {
    draw: Cell<u32>,
}

impl DiagCount {
    const fn new() -> Self {
        Self { draw: Cell::new(0) }
    }
}

/// 诊断日志（println + console_log + 文件 + TCP；与宿主壳 dlog 同构）。
/// 文件路径与 binding_probe 的 LOG_PATH 保持一致。
pub fn devlog(msg: &str) {
    println!("{}", msg);
    starfish::base::debug::console_log(msg);
    const LOG_PATH: &str = "/sdcard/Download/bp_log.txt";
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_PATH)
    {
        use std::io::Write;
        let _ = writeln!(f, "{}", msg);
    }
    #[cfg(target_os = "android")]
    {
        use std::io::Write;
        use std::sync::atomic::{AtomicBool, Ordering};
        // TCP 诊断通道：connect_timeout 300ms + 一次失败即拉黑
        // （无超时的阻塞 connect 在主线程 = ANR，2026-09-28 实测坑）
        static TCP_DEAD: AtomicBool = AtomicBool::new(false);
        if !TCP_DEAD.load(Ordering::Relaxed) {
            let addr: std::net::SocketAddr = "192.168.124.13:9000".parse().unwrap();
            match std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(300))
            {
                Ok(mut s) => {
                    let req = format!(
                        "POST /log HTTP/1.1\r\nHost: 192.168.124.13:9000\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        msg.len(),
                        msg
                    );
                    let _ = s.write_all(req.as_bytes());
                    let _ = s.flush();
                }
                Err(_) => TCP_DEAD.store(true, Ordering::Relaxed),
            }
        }
    }
}

// ── 事件泵（宿主 API）────────────────────────────────────────────

/// Web：首次用户手势已出现（pump() 看到 KeyDown/MouseButtonDown/Touch
/// 时置位）。自动播放策略的分界——此前的 play 类调用一律跳过（定约：
/// **不排队、静默丢弃**；脚本可用 `pygame.mixer.needs_gesture()` 自行
/// 延后触发）。原生平台无此状态。
#[cfg(target_arch = "wasm32")]
static GESTURE_SEEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Web 手势门查询：True = 手势未到，play 类调用应跳过（原生恒 false，
/// 编译期折叠）。跳过发生在 with_mixer 之前——不建混音器、不建
/// AudioContext、不占通道。debug 构建下首条跳过打诊断日志。
pub(crate) fn audio_gesture_locked() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        let locked = !GESTURE_SEEN.load(std::sync::atomic::Ordering::Relaxed);
        if locked {
            #[cfg(debug_assertions)]
            {
                static SKIPPED_LOGGED: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if !SKIPPED_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    devlog("[audio] Web：手势前 play 调用被跳过（自动播放策略；needs_gesture() 可查）");
                }
            }
        }
        locked
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// 事件泵（每帧一次，宿主壳调用）：引擎事件灌入待派发队列；
/// Quit / ESC 置位退出旗标。返回 true = 请求退出。
pub fn pump() -> bool {
    let mut quit = false;
    #[cfg(target_arch = "wasm32")]
    let mut gesture = false;
    // 手柄每帧排水（gilrs/Web 轮询型后端，无窗口事件流；just_pressed
    // 是帧间边沿——一帧恰好一次，归宿主帧管线）
    starfish::pygame::gamepad::poll();
    EVENTS.with(|q| {
        let mut q = q.borrow_mut();
        for ev in starfish::pygame::event::get() {
            match &ev {
                Event::Quit => quit = true,
                Event::KeyDown { key: KeyCode::Escape, .. } => quit = true,
                _ => {}
            }
            // Web：用户手势标记（浏览器 keydown/pointerdown 均算
            // User Activation；触屏已被合成为鼠标事件）
            #[cfg(target_arch = "wasm32")]
            if matches!(
                ev,
                Event::KeyDown { .. } | Event::MouseButtonDown { .. } | Event::Touch { .. }
            ) {
                gesture = true;
            }
            q.push_back(ev);
        }
    });
    // Web：首个用户手势解锁音频上下文（幂等）。手势前 play 均被跳过、
    // 惰性建混音器不会发生——此调用兜"脚本提前显式 init() 建了
    // suspended ctx"的场景（该 ctx 在首个手势时经此恢复）
    #[cfg(target_arch = "wasm32")]
    if gesture && !GESTURE_SEEN.swap(true, std::sync::atomic::Ordering::Relaxed) {
        starfish::base::audio::web_resume_audio();
    }
    quit
}

/// 事件排空（原生 `pygame.event.get()` 货源；引擎事件 → PyEvent 的
/// tag/键值映射在 native_pygame::PyEvent::from_engine，定约同源）。
pub(crate) fn drain_events() -> Vec<Event> {
    EVENTS.with(|q| q.borrow_mut().drain(..).collect())
}

/// 单发弹出队首事件（pygame `event.poll()` 语义——其余事件留在队列）。
pub(crate) fn poll_event() -> Option<Event> {
    EVENTS.with(|q| q.borrow_mut().pop_front())
}

/// 清空事件队列（pygame `event.clear()` 语义）。
pub(crate) fn clear_events() {
    EVENTS.with(|q| q.borrow_mut().clear());
}

// ── 渲染会话（ADR-5 with 语法）─────────────────────────────────

/// 会话开启（原生 `Screen.render()` 进入点）：
/// 立即 set_mode 后调用——RenderPass 存 thread-local。
pub(crate) fn session_begin() {
    let pass: RenderPass<'static> = display::get_screen().render();
    SESSION.with(|s| *s.borrow_mut() = Some(pass));
}

/// 会话结束（`Session.__exit__`）：finish 幂等打包（Drop 兜底，双保险
/// 不重复提交）。
pub(crate) fn session_end() {
    SESSION.with(|s| {
        if let Some(pass) = s.borrow_mut().take() {
            pass.finish();
        }
    });
}

// ── MRT 会话（pygame.targets：一次 pass 镜像写 N≥2 目标）────────

thread_local! {
    /// MRT 会话真身（与 Screen SESSION 并行——MRT 块内不嵌套 Screen 会话
    /// 由调用方约定；目标 Arc 由 KEEPALIVE 保活至 mrt_end）
    static MRT_SESSION: RefCell<Option<RenderPass<'static>>> = const { RefCell::new(None) };
    /// 目标保活表（render_targets_arc 的生命周期证据）
    static MRT_KEEPALIVE: RefCell<Vec<std::sync::Arc<starfish::pygame::render::Surface>>> =
        const { RefCell::new(Vec::new()) };
}

/// MRT 会话开启（`pygame.targets([...])` 进入点）：Arc 保活 + 'static 化。
pub(crate) fn mrt_begin(targets: &[std::sync::Arc<starfish::pygame::render::Surface>]) {
    let pass = starfish::pygame::render::render_targets_arc(targets);
    MRT_KEEPALIVE.with(|k| *k.borrow_mut() = targets.to_vec());
    MRT_SESSION.with(|s| *s.borrow_mut() = Some(pass));
}

/// MRT 会话结束（`__exit__`）：finish + 清保活表（顺序不可换——finish
/// 内部仍触碰目标）。
pub(crate) fn mrt_end() {
    MRT_SESSION.with(|s| {
        if let Some(pass) = s.borrow_mut().take() {
            pass.finish();
        }
    });
    MRT_KEEPALIVE.with(|k| k.borrow_mut().clear());
}

/// MRT 会话内绘制路由（DrawTarget 全家经此进入；无活跃 MRT 会话报错）。
pub(crate) fn with_mrt_session<T>(
    vm: &VirtualMachine,
    f: impl FnOnce(&RenderPass<'static>) -> T,
) -> PyResult<T> {
    MRT_SESSION.with(|s| match s.borrow().as_ref() {
        Some(pass) => Ok(f(pass)),
        None => Err(vm.new_value_error(
            "无活跃 MRT 会话（须在 `with pygame.targets([...])` 块内调用）",
        )),
    })
}

/// 会话内 blit（原生 `Session.text` 的真身：font.render 的 Surface 贴入
/// 活跃会话，返回目标 Rect）。
pub(crate) fn session_blit(
    src: &starfish::pygame::render::Surface,
    dest: (i32, i32),
    vm: &VirtualMachine,
) -> PyResult<starfish::pygame::Rect> {
    with_session(vm, |t| t.blit(src, dest))
}

/// 在活跃会话上执行绘制（无会话 = 调用方用法错误，报类型错误）
pub(crate) fn with_session<T>(
    vm: &VirtualMachine,
    f: impl FnOnce(&RenderPass<'static>) -> T,
) -> PyResult<T> {
    SESSION.with(|s| match s.borrow().as_ref() {
        Some(pass) => Ok(f(pass)),
        None => Err(vm.new_type_error(
            "draw 需在 `with screen.render():` 会话内调用（ADR-5）",
        )),
    })
}

// ── 绘制真身（FromArgs 结构体收参，批次 A 整形）─────────────────
//
// 签名即文档：颜色 (r,g,b[,a])、rect (x,y,w,h)、点 (x,y) 的序列整形在
// args 层的 TryFromObject 完成；width 缺省语义在参数层定约。消费方 =
// 原生 pygame 模块的 Session pymethod + 本文件的全局注册面。

/// [`SfColor`] → starfish [`Color`]（绘制原语/离屏会话共用）。
// ── with_batch 直通口（低层几何：UV 子区域贴图 / 任意四角）────────

use starfish::pygame::render::DrawTarget as BdTarget;

/// blit_area 主体：UV 子区域四边形进目标 batch（三会话共用；src 须为
/// PySurface，area 为源面像素坐标子区域，color 缺省白 = 原样）。
pub(crate) fn blit_area<T: BdTarget>(
    target: &T,
    src_obj: &PyObjectRef,
    dest: SfPoint,
    area: SfRect,
    color: Option<SfColor>,
    vm: &VirtualMachine,
) -> PyResult<starfish::pygame::Rect> {
    let src = src_obj
        .downcast_ref::<crate::rpy::native_pygame::PySurface>()
        .ok_or_else(|| vm.new_type_error("src 须为 pygame.Surface 实例"))?;
    let (sw, sh) = src.size();
    if area.w <= 0 || area.h <= 0 || sw == 0 || sh == 0 {
        return Err(vm.new_value_error("area 尺寸须为正且源面非空"));
    }
    // 源面像素坐标 → 归一 UV
    let uv = [
        area.x as f32 / sw as f32,
        area.y as f32 / sh as f32,
        (area.x + area.w) as f32 / sw as f32,
        (area.y + area.h) as f32 / sh as f32,
    ];
    let raw = color.map(|c| c.0).unwrap_or([255, 255, 255, 255]);
    let tint = [
        raw[0] as f32 / 255.0,
        raw[1] as f32 / 255.0,
        raw[2] as f32 / 255.0,
        raw[3] as f32 / 255.0,
    ];
    target.with_batch(|b| {
        b.push_quad_uv(
            dest.0 as f32,
            dest.1 as f32,
            area.w as f32,
            area.h as f32,
            uv,
            tint,
            Some(src.surface.texture()),
        )
    });
    Ok(starfish::pygame::Rect::new(dest.0, dest.1, area.w, area.h))
}

/// vertices 主体：原始顶点入目标 batch（逃逸舱——topo
/// "triangles"/"lines"；points = [(x, y), ...]（z=0、uv 中心、统一色）；
/// 纯色路径，贴图变体随需再加）。
pub(crate) fn vertices<T: BdTarget>(
    target: &T,
    args: &crate::rpy::args::VerticesArgs,
    vm: &VirtualMachine,
) -> PyResult<()> {
    let topo = match args.topo.as_str() {
        "triangles" => starfish::pygame::render::BatchTopology::Triangles,
        "lines" => starfish::pygame::render::BatchTopology::Lines,
        other => {
            return Err(vm.new_value_error(format!(
                "topo 须为 'triangles' 或 'lines'：{other:?}"
            )))
        }
    };
    let raw = args.color.map(|c| c.0).unwrap_or([255, 255, 255, 255]);
    let tint = [
        raw[0] as f32 / 255.0,
        raw[1] as f32 / 255.0,
        raw[2] as f32 / 255.0,
        raw[3] as f32 / 255.0,
    ];
    let verts: Vec<starfish::pygame::render::SpriteVertex> = args
        .points
        .0
        .iter()
        .map(|(x, y)| starfish::pygame::render::SpriteVertex {
            pos: [*x as f32, *y as f32, 0.0],
            uv: [0.5, 0.5],
            color: tint,
        })
        .collect();
    target.with_batch(|b| {
        b.push_vertices(topo, None, &verts);
    });
    Ok(())
}

/// geometry 主体：索引几何入目标 batch（kind "filled"/"line"——
/// push_geometry 内做 GLES 兼容的退化四边形展开；points = [(x, y), ...]
/// z=0；indices 空 = 逐 3 顶点一切三角）。
pub(crate) fn geometry<T: BdTarget>(
    target: &T,
    args: &crate::rpy::args::GeometryArgs,
    vm: &VirtualMachine,
) -> PyResult<()> {
    let kind = match args.kind.as_str() {
        "filled" => starfish::pygame::render::GeometryKind::Filled,
        "line" => starfish::pygame::render::GeometryKind::Line,
        other => {
            return Err(vm.new_value_error(format!(
                "kind 须为 'filled' 或 'line'：{other:?}"
            )))
        }
    };
    let raw = args.color.map(|c| c.0).unwrap_or([255, 255, 255, 255]);
    let tint = [
        raw[0] as f32 / 255.0,
        raw[1] as f32 / 255.0,
        raw[2] as f32 / 255.0,
        raw[3] as f32 / 255.0,
    ];
    let verts: Vec<starfish::base::gfx::ShapeVertex> = args
        .points
        .0
        .iter()
        .map(|(x, y)| starfish::base::gfx::ShapeVertex::new([*x as f32, *y as f32, 0.0], tint))
        .collect();
    let indices: Vec<u16> = match &args.indices {
        Some(o) => vm.extract_elements_with(o, |v| u16::try_from_object(vm, v.clone()))?,
        None => Vec::new(),
    };
    let geom = starfish::base::gfx::Geometry::indexed(verts, indices);
    target.with_batch(|b| b.push_geometry(kind, &geom, [0.0, 0.0], tint));
    Ok(())
}

/// quad_corners 主体：任意四角贴图进目标 batch（corners/uvs 各 4 组，
/// uv 归一 0..1；顺序 TL/TR/BR/BL）。
pub(crate) fn quad_corners<T: BdTarget>(
    target: &T,
    args: QuadCornersArgs,
    vm: &VirtualMachine,
) -> PyResult<()> {
    let src = args
        .texture
        .downcast_ref::<crate::rpy::native_pygame::PySurface>()
        .ok_or_else(|| vm.new_type_error("texture 须为 pygame.Surface 实例"))?;
    if args.corners.0.len() != 4 || args.uvs.0.len() != 4 {
        return Err(vm.new_value_error(format!(
            "corners/uvs 各须 4 组点，实际 {}/{}",
            args.corners.0.len(),
            args.uvs.0.len()
        )));
    }
    let pt = |p: [(f32, f32); 4]| -> [[f32; 2]; 4] {
        [p[0].into(), p[1].into(), p[2].into(), p[3].into()]
    };
    let corners = pt(args.corners.0.try_into().unwrap());
    let uvs = pt(args.uvs.0.try_into().unwrap());
    let c = color_of(args.color);
    target.with_batch(|b| {
        b.push_quad_corners(
            corners,
            uvs,
            [
                c.r as f32 / 255.0,
                c.g as f32 / 255.0,
                c.b as f32 / 255.0,
                c.a as f32 / 255.0,
            ],
            Some(src.surface.texture()),
        )
    });
    Ok(())
}

pub(crate) fn color_of(c: SfColor) -> Color {
    Color::new(c.0[0], c.0[1], c.0[2], c.0[3])
}

/// rect(color, rect, width=0)
pub(crate) fn sf_draw_rect(args: DrawRectArgs, vm: &VirtualMachine) -> PyResult<()> {
    DIAG.with(|d| {
        let n = d.draw.get() + 1;
        d.draw.set(n);
        if n == 1 {
            devlog(&format!(
                "[sf] draw_rect 首调 rect=({},{},{},{}) rgba={:?} width={:?}",
                args.rect.x, args.rect.y, args.rect.w, args.rect.h, args.color.0, args.width
            ));
        }
    });
    let color = color_of(args.color);
    let width = args.width.unwrap_or(0).max(0) as u32;
    with_session(vm, |t| {
        draw::rect(t, color, Rect::new(args.rect.x, args.rect.y, args.rect.w, args.rect.h), width);
    })
}

/// circle(color, center, radius, width=0)
pub(crate) fn sf_draw_circle(args: DrawCircleArgs, vm: &VirtualMachine) -> PyResult<()> {
    let color = color_of(args.color);
    let radius = args.radius.max(0) as u32;
    let width = args.width.unwrap_or(0).max(0) as u32;
    with_session(vm, |t| {
        draw::circle(t, color, (args.center.0, args.center.1), radius, width);
    })
}

/// line(color, start, end, width=1)
pub(crate) fn sf_draw_line(args: DrawLineArgs, vm: &VirtualMachine) -> PyResult<()> {
    let color = color_of(args.color);
    let width = args.width.unwrap_or(1).max(1) as u32;
    with_session(vm, |t| {
        draw::line(t, color, (args.start.0, args.start.1), (args.end.0, args.end.1), width);
    })
}

/// polygon(color, points, width=0)——points = [(x, y), ...]
pub(crate) fn sf_draw_polygon(args: DrawPolygonArgs, vm: &VirtualMachine) -> PyResult<()> {
    let color = color_of(args.color);
    let width = args.width.unwrap_or(0).max(0) as u32;
    with_session(vm, |t| {
        draw::polygon(t, color, &args.points.0, width);
    })
}

/// ellipse(color, rect, width=0)
pub(crate) fn sf_draw_ellipse(args: DrawEllipseArgs, vm: &VirtualMachine) -> PyResult<()> {
    let color = color_of(args.color);
    let width = args.width.unwrap_or(0).max(0) as u32;
    with_session(vm, |t| {
        draw::ellipse(t, color, Rect::new(args.rect.x, args.rect.y, args.rect.w, args.rect.h), width);
    })
}

/// arc(color, rect, start_angle, stop_angle, width=1)——角度弧度制
/// （pygame draw.arc 语义；starfish v1 描边恒 1px，width 收下不生效）
pub(crate) fn sf_draw_arc(args: DrawArcArgs, vm: &VirtualMachine) -> PyResult<()> {
    let color = color_of(args.color);
    let width = args.width.unwrap_or(1).max(1) as u32;
    with_session(vm, |t| {
        draw::arc(
            t,
            color,
            Rect::new(args.rect.x, args.rect.y, args.rect.w, args.rect.h),
            args.start_angle.into_float() as f32,
            args.stop_angle.into_float() as f32,
            width,
        );
    })
}

/// fill(color)：全局屏填充（`Screen.fill` / 会话 `fill` 的真身；
/// 与旧扁平 `sf_fill(r,g,b,a)` 同语义，颜色形态换成 pygame tuple）。
pub(crate) fn sf_draw_fill(args: FillArgs, _vm: &VirtualMachine) -> PyResult<()> {
    display::get_screen().fill(color_of(args.color));
    Ok(())
}

// ── 注册（探针面 + 旧别名；见文件头"注册面"说明）────────────────

/// 注入脚本全局（幂等）。
pub fn register(vm: &VirtualMachine, scope: &Scope) {
    let items = [
        ("sf_draw_rect", vm.new_function("sf_draw_rect", sf_draw_rect).into()),
        (
            "sf_draw_circle",
            vm.new_function("sf_draw_circle", sf_draw_circle).into(),
        ),
        ("sf_draw_line", vm.new_function("sf_draw_line", sf_draw_line).into()),
        (
            "sf_draw_polygon",
            vm.new_function("sf_draw_polygon", sf_draw_polygon).into(),
        ),
        (
            "sf_draw_ellipse",
            vm.new_function("sf_draw_ellipse", sf_draw_ellipse).into(),
        ),
        (
            "sf_draw_arc",
            vm.new_function("sf_draw_arc", sf_draw_arc).into(),
        ),
    ];
    for (name, obj) in items {
        scope
            .globals
            .set_item(name, obj, vm)
            .expect("bridge 注册：set_item 失败");
    }
}

// ── 字体字节缓存（批次 G）────────────────────────────────────────

/// CJK 等大字体的字节缓存：宿主壳启动时经 `base::io::read` 预加载
/// （桌面 fs / Web fetch / Android fs 三后端），Font 构造同步命中——
/// 避免在 Python 同步构造器里 await 异步 IO。
thread_local! {
    static FONT_CACHE: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
}

/// 宿主壳预加载字体字节（按名缓存；Font 构造按同名命中）
pub fn font_cache_set(name: &str, bytes: Vec<u8>) {
    FONT_CACHE.with(|c| c.borrow_mut().insert(name.to_owned(), bytes));
}

pub(crate) fn font_cache_get(name: &str) -> Option<Vec<u8>> {
    FONT_CACHE.with(|c| c.borrow().get(name).cloned())
}

// ── 音频桥（批次 E）──────────────────────────────────────────────

/// 内嵌提示音（starfish 标准测试音，三平台零 IO；Sound() 无参构造用）
const BEEP: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../starfish/resources/audio/bleep.wav"
));

thread_local! {
    /// 音频混音器（构造即开设备、Drop 即停 → 必须保活；Send 非 Sync →
    /// thread-local，与 SESSION/EVENTS 同格局）
    static MIXER: RefCell<Option<AudioMixer>> = const { RefCell::new(None) };
}

fn audio_err(vm: &VirtualMachine, e: impl std::fmt::Debug) -> PyBaseExceptionRef {
    vm.new_value_error(format!("音频错误: {e:?}"))
}

/// 访问混音器（未 init 时以默认 8 通道自动创建——比 pygame 的 init 门槛友好）
pub(crate) fn with_mixer<R>(
    vm: &VirtualMachine,
    f: impl FnOnce(&mut AudioMixer) -> R,
) -> PyResult<R> {
    MIXER.with(|m| -> PyResult<R> {
        let mut slot = m.borrow_mut();
        if slot.is_none() {
            *slot = Some(
                AudioMixer::new(8)
                    .map_err(|e| vm.new_value_error(format!("音频设备初始化失败: {e:?}")))?,
            );
        }
        Ok(f(slot.as_mut().expect("刚初始化")))
    })
}

/// pygame.mixer.init(channels)：显式（重建）初始化
pub fn mixer_init(channels: u32, vm: &VirtualMachine) -> PyResult<()> {
    MIXER.with(|m| -> PyResult<()> {
        *m.borrow_mut() = Some(
            AudioMixer::new(channels.max(1))
                .map_err(|e| vm.new_value_error(format!("音频设备初始化失败: {e:?}")))?,
        );
        Ok(())
    });
    Ok(())
}

/// pygame.mixer.quit()：Drop 即停设备
pub fn mixer_quit() {
    MIXER.with(|m| *m.borrow_mut() = None);
}

/// pygame.mixer.get_init()：bool 形态（真 pygame 返回参数三元组/None）
pub fn mixer_get_init() -> bool {
    MIXER.with(|m| m.borrow().is_some())
}

/// Web 每帧驱动流式解码（无解码线程；桌面 no-op——解码线程自驱）。
/// 宿主壳帧循环每帧调用一次。
pub fn audio_pump() {
    #[cfg(target_arch = "wasm32")]
    MIXER.with(|m| {
        if let Some(mixer) = m.borrow_mut().as_mut() {
            mixer.pump_streams(2048);
        }
    });
}

/// 音频源对象 → SoundData（None = 内嵌提示音 / str = 文件路径 /
/// bytes = 内存解码；OGG/MP3/FLAC/WAV 按内容探测）
pub(crate) fn sound_source_to_data(
    source: Option<PyObjectRef>,
    vm: &VirtualMachine,
) -> PyResult<Arc<SoundData>> {
    use rustpython_vm::convert::TryFromObject;
    match source {
        // Font(None) 惯例延续：无参 = 内嵌标准提示音（已知合法，expect 安全）
        None => Ok(Arc::new(
            SoundData::from_bytes(BEEP.to_vec()).expect("内嵌提示音解码失败"),
        )),
        Some(o) => {
            if let Some(s) = o.downcast_ref::<rustpython_vm::builtins::PyStr>() {
                let t = s
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("音频路径含非 UTF-8 字符"))?;
                SoundData::from_file(t)
            } else {
                // bytes / bytearray / memoryview（try_bytes_like 对非字节对象
                // 自报 TypeError）
                o.try_bytes_like(vm, |b| SoundData::from_bytes(b.to_vec()))?
            }
        }
        .map(Arc::new)
        .map_err(|e| audio_err(vm, e))
    }
}

// ── 提取辅助 ─────────────────────────────────────────────────────

fn rgba(r: i32, g: i32, b: i32, a: i32) -> [f32; 4] {
    [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ]
}
