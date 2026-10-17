//! pygame 原生模块（批次 B）：`#[pymodule]` 树 + pyclass，`pygame_shim.py` 退役。
//!
//! 分层（"能力住 Rust"定约的完成形态）：
//! - **模块树原生构建**：`import pygame` 直达 PyModuleDef——多阶段 init
//!   （`create_module` + `exec_module` 挂子模块）后注入 sys.modules
//!   （[`install`]，宿主壳在 `interp.enter` 内调用）；
//! - **Screen / Session / Event 三个 pyclass**：with 会话协议、绘制方法、
//!   事件对象全部原生——参数整形复用 `args` 层（FromArgs 收参，kwargs /
//!   缺省值与批次 A 签名完全一致）；
//! - 会话（thread-local RenderPass）、事件队列、诊断计数等引擎侧逻辑仍在
//!   [`super::bridge`]，本文件只做 Python 面——零 RP 状态、零 starfish 反向感知。
//!
//! 定约不变：无 `pygame.draw.*(screen, ...)` 兼容形态（架构 §九）；
//! `set_mode` 返回占位 Screen（真身由宿主壳在脚本前以目标尺寸装配）。
//!
//! 宏用法备忘（本次实测，防重蹈）：
//! - `pyattr`/`pyfunction`/`pymethod`/`pygetset` 是被 `#[pymodule]` /
//!   `#[pyclass]` 宏消费的**内部标记**，无需 import；真实展开器只有
//!   `pyclass` / `pymodule`（属性宏）与 `FromArgs`（derive）；
//! - **子模块必须嵌套声明**在父 mod 内（`#[pymodule]`）——父宏只为
//!   嵌套 Item::Mod 生成挂载代码；`with(...)` 是"协议/属性并入父模块"
//!   形态，不是子模块挂载；
//! - `with(协议)` 挂在 **impl** 的 `#[pyclass]` 上，不挂在 struct 上；
//! - struct 内的 `#[pyclass]` 要么配 `#[pyattr]`（注册进模块），要么
//!   `no_attr`（自由类型）。

use rustpython_derive::pymodule;
use rustpython_vm::VirtualMachine;

// 转出给 args 层（SfRect 直通）与 pygame_impls（方法实现层）
pub(crate) use pygame::PyColor;
pub(crate) use pygame::PyRect;
pub(crate) use pygame::PySurface;
pub(crate) use pygame::mixer::PySound;

/// 多阶段 init 后注入 sys.modules（`import pygame` 的直达路径）。
/// **必须在 `interp.enter` 内、essential init 完成后调用**（init_hook 时点
/// 调用会递归爆栈——解释器初始化深处动模块机制的实测教训，见
/// `rpy::new_interpreter` 的生命周期说明）。
pub fn install(vm: &VirtualMachine) {
    use pygame::module_def;
    if let Err(msg) = try_install(vm, module_def(&vm.ctx)) {
        panic!("pygame 原生模块注入失败: {msg}");
    }
}

/// Python 异常 → 可读文本（PyBaseException 无 Display，repr 转）。
fn exc_msg(vm: &VirtualMachine, e: rustpython_vm::builtins::PyBaseExceptionRef) -> String {
    let obj: rustpython_vm::PyObjectRef = e.into();
    obj.repr(vm)
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "<unrepr-able exception>".to_owned())
}

fn try_install(vm: &VirtualMachine, def: &'static rustpython_vm::builtins::PyModuleDef) -> Result<(), String> {
    let module = def
        .create_module(vm)
        .map_err(|e| format!("create_module: {}", exc_msg(vm, e)))?;
    def.exec_module(vm, &module)
        .map_err(|e| format!("exec_module: {}", exc_msg(vm, e)))?; // 挂子模块
    let sys_modules = vm
        .sys_module
        .get_attr("modules", vm)
        .map_err(|e| format!("sys.modules fetch: {}", exc_msg(vm, e)))?;
    sys_modules
        .set_item("pygame", module.clone().into(), vm)
        .map_err(|e| format!("sys.modules inject: {}", exc_msg(vm, e)))?;
    // 子模块补进 sys.modules（`import pygame.display` 直达；属性访问已被
    // exec 挂好，此处为 import 系统补账）
    for (attr, full) in [
        ("display", "pygame.display"),
        ("event", "pygame.event"),
        ("time", "pygame.time"),
        ("font", "pygame.font"),
        ("mixer", "pygame.mixer"),
        ("transform", "pygame.transform"),
        ("image", "pygame.image"),
        ("mask", "pygame.mask"),
        ("permission", "pygame.permission"),
        ("video", "pygame.video"),
    ] {
        let child = module
            .get_attr(attr, vm)
            .map_err(|e| format!("child {attr}: {}", exc_msg(vm, e)))?;
        sys_modules
            .set_item(full, child, vm)
            .map_err(|e| format!("child inject {full}: {}", exc_msg(vm, e)))?;
    }
    Ok(())
}

#[pymodule(name = "pygame")]
pub(crate) mod pygame {
    use rustpython_derive::{pyclass, pymodule};
    use std::cell::Cell;
    use rustpython_vm::function::{FuncArgs, PyComparisonValue};
    use rustpython_vm::types::{Comparable, PyComparisonOp, Representable};
    use rustpython_vm::{
        PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine,
        builtins::{PyModule, PyStr, PyStrRef},
    };
    use starfish::pygame::Rect as EngineRect;
    use starfish::pygame::render::{DrawTarget, Surface as SfSurface};
    use crate::rpy::{bridge, args};
    use crate::rpy::args::SfColor;

    /// Session 绘制方法的 Rect 形参别名（args::SfRect 的直通分支消费方）
    type PyRectRef = PyRef<PyRect>;

    // ── 默认字体（顶层常量——维护者/开发者自定义入口）──────────────
    // 内嵌 Antonio-Regular 47KB（拉丁，OFL 许可，跨仓库 include_bytes 单源）。
    // 开发者可整体替换此路径换默认字体：如英语使用者不依赖 CJK 预载链，
    // 将 include_bytes 指向自有拉丁字体即可（减体积/去字体文件依赖）。
    // 注：CJK 中文走宿主 io 预载（msyh/cjk.ttf/系统候选链），与此常量无关。
    pub(crate) const DEFAULT_FONT: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../starfish/resources/fonts/Antonio-Regular.ttf"
    ));

    // ── Surface：starfish Surface 直通句柄（批次 F）─────────────────
    // payload = owned starfish Surface（无 Clone/Debug/生命周期参数；
    // 全 API &self 内部可变性）。构造/使用须在 display init 后（GPU 槽）。
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame", name = "Surface", unhashable = true)]
    pub(crate) struct PySurface {
        /// Arc 共享：render() 会话对象与本体共享同一 Surface（离屏批量绘制）
        pub(crate) surface: std::sync::Arc<SfSurface>,
    }

    impl PySurface {
        /// 会话/transform/image 共用的尺寸读取
        pub(crate) fn size(&self) -> (u32, u32) {
            self.surface.size()
        }
    }

    impl std::fmt::Debug for PySurface {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let (w, h) = self.size();
            f.write_str(&format!("Surface({}x{})", w, h))
        }
    }

    // ── Surface 会话：离屏绘制目标（与 Screen 会话同语法）─────────
    // Surface 有单一内部 batch——多次绘制经 with_batch 自然累积，
    // __exit__ flush 打包上 GPU（无 Screen 的通道拆分问题）。
    #[derive(PyPayload)]
    #[pyclass(no_attr, module = false, name = "SurfaceSession", unhashable = true)]
    pub(crate) struct PySurfaceSession {
        pub(crate) surface: std::sync::Arc<SfSurface>,
    }

    impl std::fmt::Debug for PySurfaceSession {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("SurfaceSession(<surface>)")
        }
    }

    #[pyclass]
    impl PySurfaceSession {
        #[pymethod]
        fn __enter__(zelf: PyRef<Self>) -> PyRef<Self> {
            zelf
        }

        /// 会话收口：flush 内部 batch 上 GPU（无 present——离屏）
        #[pymethod]
        fn __exit__(&self, _exc_info: FuncArgs, vm: &VirtualMachine) -> bool {
            self.surface.flush();
            let _ = vm;
            false
        }

        #[pymethod]
        fn fill(&self, color: crate::rpy::args::SfColor, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            use starfish::pygame::render::DrawTarget;
            let r = DrawTarget::fill(&*self.surface, [color.0[0], color.0[1], color.0[2], color.0[3]]);
            Ok(PyRect(Cell::new(r)).into_ref(&vm.ctx).into())
        }

        #[pymethod]
        fn rect(&self, args: args::DrawRectArgs, vm: &VirtualMachine) -> PyResult<()> {
            starfish::pygame::draw::rect(
                &*self.surface,
                bridge::color_of(args.color),
                starfish::pygame::Rect::new(args.rect.x, args.rect.y, args.rect.w, args.rect.h),
                args.width.unwrap_or(0).max(0) as u32,
            );
            Ok(())
        }

        #[pymethod]
        fn circle(&self, args: args::DrawCircleArgs, vm: &VirtualMachine) -> PyResult<()> {
            starfish::pygame::draw::circle(
                &*self.surface,
                bridge::color_of(args.color),
                (args.center.0, args.center.1),
                args.radius.max(0) as u32,
                args.width.unwrap_or(0).max(0) as u32,
            );
            Ok(())
        }

        #[pymethod]
        fn line(&self, args: args::DrawLineArgs, vm: &VirtualMachine) -> PyResult<()> {
            starfish::pygame::draw::line(
                &*self.surface,
                bridge::color_of(args.color),
                (args.start.0, args.start.1),
                (args.end.0, args.end.1),
                args.width.unwrap_or(1).max(1) as u32,
            );
            Ok(())
        }

        #[pymethod]
        fn polygon(&self, args: args::DrawPolygonArgs, vm: &VirtualMachine) -> PyResult<()> {
            starfish::pygame::draw::polygon(
                &*self.surface,
                bridge::color_of(args.color),
                &args.points.0,
                args.width.unwrap_or(0).max(0) as u32,
            );
            Ok(())
        }

        #[pymethod]
        fn ellipse(&self, args: args::DrawEllipseArgs, vm: &VirtualMachine) -> PyResult<()> {
            starfish::pygame::draw::ellipse(
                &*self.surface,
                bridge::color_of(args.color),
                starfish::pygame::Rect::new(args.rect.x, args.rect.y, args.rect.w, args.rect.h),
                args.width.unwrap_or(0).max(0) as u32,
            );
            Ok(())
        }

        #[pymethod]
        fn arc(&self, args: args::DrawArcArgs, vm: &VirtualMachine) -> PyResult<()> {
            starfish::pygame::draw::arc(
                &*self.surface,
                bridge::color_of(args.color),
                starfish::pygame::Rect::new(args.rect.x, args.rect.y, args.rect.w, args.rect.h),
                args.start_angle.into_float() as f32,
                args.stop_angle.into_float() as f32,
                args.width.unwrap_or(1).max(1) as u32,
            );
            Ok(())
        }

        #[pymethod]
        fn blit(
            &self,
            src: PyRef<PySurface>,
            dest: args::SfPoint,
            vm: &VirtualMachine,
        ) -> PyResult<PyObjectRef> {
            use starfish::pygame::render::DrawTarget;
            let r = DrawTarget::blit(&*self.surface, &src.surface, (dest.0, dest.1));
            Ok(PyRect(Cell::new(r)).into_ref(&vm.ctx).into())
        }

        /// blit_area(src, dest, area, color=None) -> Rect：UV 子区域贴图
        /// （with_batch push_quad_uv 直通——源面像素坐标子区域一次四边形）
        #[pymethod]
        fn blit_area(&self, args: args::BlitAreaArgs, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let r = bridge::blit_area(&*self.surface, &args.src, args.dest, args.area, args.color, vm)?;
            Ok(PyRect(Cell::new(r)).into_ref(&vm.ctx).into())
        }

        /// quad(texture, corners, uvs, color)：任意四角贴图（with_batch
        /// push_quad_corners 直通——角点/uv 各 4 组，uv 归一 0..1）
        #[pymethod]
        fn quad(&self, args: args::QuadCornersArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::quad_corners(&*self.surface, args, vm)
        }

        /// vertices(topo, points, color)：原始顶点入批（逃逸舱——topo
        /// "triangles"/"lines"；points = [(x, y), ...]；z=0、uv 中心、统一色）
        #[pymethod]
        fn vertices(&self, args: args::VerticesArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::vertices(&*self.surface, &args, vm)
        }

        /// geometry(kind, points, indices, color)：索引几何入批
        ///（kind "filled"/"line"；indices 空 = 逐 3 顶点一切三角；
        /// push_geometry 内做 GLES 兼容退化四边形展开）
        #[pymethod]
        fn geometry(&self, args: args::GeometryArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::geometry(&*self.surface, &args, vm)
        }

        /// text(font, text, dest, color) -> Rect：会话内直接绘制文字
        /// （目标 = 本 Surface；font.render + blit 融合，批次 F+）
        #[pymethod]
        fn text(
            &self,
            font: PyRef<font::PyFont>,
            text: PyStrRef,
            dest: args::SfPoint,
            color: crate::rpy::args::SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<PyObjectRef> {
            use rustpython_vm::convert::TryFromObject;
            use starfish::pygame::render::DrawTarget;
            let t = text
                .to_str()
                .ok_or_else(|| vm.new_value_error("文本含非 UTF-8 字符"))?;
            let rendered = font
                .inner
                .render(t, true, [color.0[0], color.0[1], color.0[2], color.0[3]]);
            let r = DrawTarget::blit(&*self.surface, &rendered, (dest.0, dest.1));
            Ok(PyRect(Cell::new(r)).into_ref(&vm.ctx).into())
        }
    }




    /// 经典全量语义占位：引擎初始化由宿主壳完成，返回 (0, 0)。
    #[pyfunction]
    fn init() -> (usize, usize) {
        (0, 0)
    }

    /// 占位（引擎收口由宿主壳完成）。
    #[pyfunction]
    fn quit() {}

    // ── Screen：set_mode 返回的绘制终点占位（真身 = Rust 全局 Screen，
    //    由宿主壳装配）──
    #[derive(Debug, PyPayload)]
    #[pyclass(no_attr, module = false, name = "Screen", unhashable = true)]
    pub(crate) struct PyScreen;

    #[pyclass]
    impl PyScreen {
        /// ADR-5 with 会话入口：进入 = 开启 RenderPass，退出 = finish 打包。
        #[pymethod]
        fn render(&self, vm: &VirtualMachine) -> PyResult<PyRef<PyScreenSession>> {
            bridge::session_begin();
            Ok(PyScreenSession.into_ref(&vm.ctx))
        }

        /// 深度会话（v1.1：Surface 创建整合 depth_buffer 后启用，架构 §九）。
        #[pymethod]
        fn render_depth(&self, vm: &VirtualMachine) -> PyResult<()> {
            Err(vm.new_not_implemented_error("render_depth 随 Surface 批次（v1.1）落地"))
        }

        // /// 全局屏填充（真身 bridge::sf_draw_fill——批次 A 签名）。
        // #[pymethod]
        // fn fill(&self, args: FillArgs, vm: &VirtualMachine) -> PyResult<()> {
        //     bridge::sf_draw_fill(args, vm)
        // }
    }

    // ── Session：绘制会话——唯一绘制入口（无 pygame.draw.*(screen, ...)
    //    兼容形态，架构 §九）。方法签名即 bridge.rs 契约（缺省值/序列整形在
    //    args 层）：
    //    rect(color, rect, width=0) / circle(color, center, radius, width=0)
    //    line(color, start, end, width=1) / polygon(color, points, width=0)
    //    fill(color) ──
    #[derive(Debug, PyPayload)]
    #[pyclass(no_attr, module = false, name = "Session", unhashable = true)]
    pub(crate) struct PyScreenSession;

    #[pyclass]
    impl PyScreenSession {
        #[pymethod]
        fn __enter__(zelf: PyRef<Self>) -> PyRef<Self> {
            zelf
        }

        /// 异常不吞（返回 False 交回宿主壳打印收口）。
        #[pymethod]
        fn __exit__(&self, _exc_info: FuncArgs) -> bool {
            bridge::session_end();
            false
        }

        #[pymethod]
        fn fill(&self, args: FillArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::sf_draw_fill(args, vm)
        }

        #[pymethod]
        fn rect(&self, args: args::DrawRectArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::sf_draw_rect(args, vm)
        }

        #[pymethod]
        fn circle(&self, args: args::DrawCircleArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::sf_draw_circle(args, vm)
        }

        #[pymethod]
        fn line(&self, args: args::DrawLineArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::sf_draw_line(args, vm)
        }

        #[pymethod]
        fn polygon(&self, args: args::DrawPolygonArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::sf_draw_polygon(args, vm)
        }

        #[pymethod]
        fn ellipse(&self, args: args::DrawEllipseArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::sf_draw_ellipse(args, vm)
        }

        #[pymethod]
        fn arc(&self, args: args::DrawArcArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::sf_draw_arc(args, vm)
        }

        /// blit(src, dest) -> Rect：Surface 贴入会话（批次 F）
        #[pymethod]
        fn blit(&self, src: PyRef<PySurface>, dest: args::SfPoint, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let rect = bridge::session_blit(&src.surface, (dest.0, dest.1), vm)?;
            Ok(PyRect(Cell::new(rect)).into_ref(&vm.ctx).into())
        }

        /// blit_area(src, dest, area, color=None) -> Rect：UV 子区域贴图
        /// （with_batch push_quad_uv 直通——源面像素坐标子区域一次四边形）
        #[pymethod]
        fn blit_area(&self, args: args::BlitAreaArgs, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let r = bridge::with_session(vm, |p| {
                bridge::blit_area(p, &args.src, args.dest, args.area, args.color, vm)
            })??;
            Ok(PyRect(Cell::new(r)).into_ref(&vm.ctx).into())
        }

        /// quad(texture, corners, uvs, color)：任意四角贴图（with_batch
        /// push_quad_corners 直通——角点/uv 各 4 组，uv 归一 0..1）
        #[pymethod]
        fn quad(&self, args: args::QuadCornersArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_session(vm, |p| bridge::quad_corners(p, args, vm))?;
            Ok(())
        }

        /// vertices(topo, points, color)：原始顶点入批（逃逸舱——topo
        /// "triangles"/"lines"；points = [(x, y), ...]；z=0、uv 中心、统一色）
        #[pymethod]
        fn vertices(&self, args: args::VerticesArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_session(vm, |p| bridge::vertices(p, &args, vm))?
        }

        /// geometry(kind, points, indices, color)：索引几何入批
        ///（kind "filled"/"line"；indices 空 = 逐 3 顶点一切三角；
        /// push_geometry 内做 GLES 兼容退化四边形展开）
        #[pymethod]
        fn geometry(&self, args: args::GeometryArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_session(vm, |p| bridge::geometry(p, &args, vm))?
        }

        /// text(font, text, dest, color) -> Rect：font.render + 会话 blit
        /// 融合（批次 D）。GPU 需已 init（帧循环内 ✓）；无 background；
        /// 换行符按单行语义跳过；antialias 恒开（starfish 形态）。
        #[pymethod]
        fn text(
            &self,
            font: font::PyFontRef,
            text: PyStrRef,
            dest: args::SfPoint,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<PyObjectRef> {
            let t = text
                .to_str()
                .ok_or_else(|| vm.new_value_error("文本含非 UTF-8 字符"))?;
            let surface = font
                .inner
                .render(t, true, [color.0[0], color.0[1], color.0[2], color.0[3]]);
            let rect = bridge::session_blit(&surface, (dest.0, dest.1), vm)?;
            Ok(PyRect(Cell::new(rect)).into_ref(&vm.ctx).into())
        }
    }

    // FillArgs 从 args 层引入（本 mod 内 pymethod 用）
    use args::FillArgs;

    // ── Rect：starfish Rect 直通句柄（批次 C，transform/sprite 的前置）──
    // payload = `Cell<EngineRect>`：Rect 是 Copy 且无内部指针，Cell 即可
    // 支持 *_ip 原地方法（RP pymethod 无 &mut self 形态，内置类型一律
    // 内部可变性）。构造 = pygame.Rect(left, top, width, height) 4 参。
    #[derive(Debug, PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame", name = "Rect", unhashable = true)]
    pub(crate) struct PyRect(pub(crate) Cell<EngineRect>);



    // ── Color：颜色类型（批次三十九，替代旧 fn Color 函数——starfish
    //    pygame::color::Color 直通句柄；payload = Cell<Color>（Copy，
    //    Rect 同款模式）。构造/方法在 pygame_impls；SfColor 有 downcast
    //    分支（Color 实例直接作颜色参数，与 Rect 直通同形态））──
    #[derive(Debug, PyPayload)]
    #[pyattr]
    #[pyclass(module = "pygame", name = "Color", unhashable = true)]
    pub(crate) struct PyColor(pub(crate) Cell<starfish::pygame::color::Color>);



        


    // ── 子模块（必须嵌套声明——父宏只为嵌套 Item::Mod 生成挂载代码）──
    #[pymodule]
    pub(crate) mod display {
        use super::PyScreen;
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
        pub(crate) const RESIZABLE: u32 = starfish::pygame::display::RESIZABLE;
        #[pyattr]
        pub(crate) const FULLSCREEN: u32 = starfish::pygame::display::FULLSCREEN;
        #[pyattr]
        pub(crate) const NOFRAME: u32 = starfish::pygame::display::NOFRAME;
        #[pyattr]
        pub(crate) const HIDDEN: u32 = starfish::pygame::display::HIDDEN;
        #[pyattr]
        pub(crate) const ALWAYS_ON_TOP: u32 = starfish::pygame::display::ALWAYS_ON_TOP;
        #[pyattr]
        pub(crate) const TRANSPARENT: u32 = starfish::pygame::display::TRANSPARENT;
        #[pyattr]
        pub(crate) const VULKAN: u32 = starfish::pygame::display::VULKAN;
        #[pyattr]
        pub(crate) const DIRECTX: u32 = starfish::pygame::display::DIRECTX;
        #[pyattr]
        pub(crate) const METAL: u32 = starfish::pygame::display::METAL;
        #[pyattr]
        pub(crate) const OPENGLES: u32 = starfish::pygame::display::OPENGLES;

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
            bridge::devlog(&format!(
                "[pygame] display.set_mode 需求 {:?}（真身由宿主壳装配）",
                (args.size.0, args.size.1)
            ));
            // 完整窗口请求（批次 O + 尺寸扩展）：尺寸跟随（set_size）+
            // 旗标运行时生效（apply_window_request）
            let bits = match &args.flags {
                Some(f) => flags_bits(f, vm)?,
                None => 0,
            };
            starfish::pygame::display::apply_window_request(
                (args.size.0.max(0) as u32, args.size.1.max(0) as u32),
                bits,
            );
            Ok(PyScreen.into_ref(&vm.ctx))
        }

        #[pyfunction]
        fn flip(_vm: &VirtualMachine) {
            starfish::pygame::display::flip();
        }

        /// set_opacity(v)：窗口整体不透明度（0.0 透明 .. 1.0 不透明）。
        /// 桌宠/悬浮件形态；与 TRANSPARENT 创建旗标正交（后者让 alpha 0
        /// 区域透出桌面，前者调窗口整体 alpha）。
        #[pyfunction]
        fn set_opacity(opacity: f32, _vm: &VirtualMachine) {
            starfish::pygame::display::set_opacity(opacity)
        }

        /// set_caption(title)：窗口标题。
        #[pyfunction]
        fn set_caption(title: PyStrRef, vm: &VirtualMachine) -> PyResult<()> {
            let t = title
                .to_str()
                .ok_or_else(|| vm.new_value_error("标题含非 UTF-8 字符"))?;
            starfish::pygame::display::set_caption(t);
            Ok(())
        }

        /// get_init() -> bool：set_mode 是否已调用（display 初始化态）。
        #[pyfunction]
        fn get_init(_vm: &VirtualMachine) -> bool {
            starfish::pygame::display::get_init()
        }
    }

    #[pymodule]
    pub(crate) mod event {
        use rustpython_vm::builtins::PyModule;
        use starfish::{base::window::KeyCode};
        use starfish::pygame::Event as EngineEvent;
        use rustpython_derive::{pyclass, pymodule};
        use rustpython_vm::types::{Comparable, PyComparisonOp, Representable};
        use rustpython_vm::{
            Py, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine,
            builtins::{PyStr, PyStrRef},
        };
        use crate::rpy::{bridge, args};


        // ── 常量（tag/键值定约与 bridge.rs 事件映射一致）──
        #[pyattr]
        pub(crate) const QUIT: &str = "quit";
        #[pyattr]
        pub(crate) const KEYDOWN: &str = "keydown";
        #[pyattr]
        pub(crate) const KEYUP: &str = "keyup";
        #[pyattr]
        pub(crate) const MOUSEMOTION: &str = "mousemotion";
        #[pyattr]
        pub(crate) const MOUSEBUTTONDOWN: &str = "mousebuttondown";
        #[pyattr]
        pub(crate) const MOUSEBUTTONUP: &str = "mousebuttonup";
        #[pyattr]
        pub(crate) const TEXTINPUT: &str = "textinput";



        // ── Key：类型化键值（K_* 常量的载体；引擎无整数键码表，typed 是
        //    starfish locals 的既定决策）──
        #[derive(Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame", name = "Key", unhashable = true)]
        pub(crate) struct PyKey(pub(crate) KeyCode);

        
        // ── Pressed：key.get_pressed() 的按键按下视图 ──
        // 零尺寸视图不持有（starfish Pressed 无 Debug 不满足 payload 约束），
        // 查询时实时取视图——状态本体在 key 的 thread-local，语义等价。
        #[derive(Debug, PyPayload)]
        #[pyclass(no_attr, module = false, name = "Pressed", unhashable = true)]
        pub(crate) struct PyPressed;

        #[pyclass]
        impl PyPressed {
            #[pymethod]
            fn get(&self, key: PyRef<PyKey>) -> bool {
                starfish::pygame::key::get_pressed().get(key.0)
            }
        }



        // ── Event：事件对象（pygame.event.get() 产物）。字段形态与 shim 时代
        //    一致：`type` 为 tag 字符串（QUIT 等常量同源）、`key` 承载键值
        //    （int）或文本（textinput）；match 类模式解构随 pygame.Event 可用 ──
        #[derive(Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame", name = "Event", unhashable = true)]
        pub(crate) struct PyEvent {
            pub(crate) typ: PyStrRef,
            pub(crate) key: PyObjectRef,
            pub(crate) x: i32,
            pub(crate) y: i32,
            pub(crate) button: i32,
        }

        #[pyclass(with(Representable))]
        impl PyEvent {
            #[pygetset(name = "type")]
            fn typ(&self) -> PyStrRef {
                self.typ.clone()
            }

            #[pygetset]
            fn key(&self) -> PyObjectRef {
                self.key.clone()
            }

            #[pygetset]
            fn x(&self) -> i32 {
                self.x
            }

            #[pygetset]
            fn y(&self) -> i32 {
                self.y
            }

            #[pygetset]
            fn button(&self) -> i32 {
                self.button
            }
        }

        impl Representable for PyEvent {
            fn repr(zelf: &Py<Self>, vm: &VirtualMachine) -> PyResult<PyRef<PyStr>> {
                Ok(vm.ctx.new_str(format!(
                    "<Event {} key={} pos=({}, {}) button={}>",
                    zelf.typ,
                    zelf.key.repr(vm)?,
                    zelf.x,
                    zelf.y,
                    zelf.button
                )))
            }
        }

        impl PyEvent {
            /// 引擎事件 → PyEvent（typed 键值定约，批次 C：`e.key == pygame.K_x`
            /// 用 Key 实例值相等比较；非键事件的 key 位为 int 0）。
            pub(crate) fn from_engine(vm: &VirtualMachine, ev: EngineEvent) -> Self {
                let tag = |s: &str| -> PyStrRef { vm.ctx.intern_str(s).to_owned() };
                let int_obj = |v: u32| -> PyObjectRef { vm.ctx.new_int(v).into() };
                let key_obj = |k: KeyCode| -> PyObjectRef { PyKey(k).into_pyobject(vm) };
                match ev {
                    EngineEvent::Quit => Self {
                        typ: tag("quit"),
                        key: int_obj(0),
                        x: 0,
                        y: 0,
                        button: 0,
                    },
                    EngineEvent::KeyDown { key, .. } => Self {
                        typ: tag("keydown"),
                        key: key_obj(key),
                        x: 0,
                        y: 0,
                        button: 0,
                    },
                    EngineEvent::KeyUp { key } => Self {
                        typ: tag("keyup"),
                        key: key_obj(key),
                        x: 0,
                        y: 0,
                        button: 0,
                    },
                    EngineEvent::MouseMotion { x, y } => Self {
                        typ: tag("mousemotion"),
                        key: int_obj(0),
                        x,
                        y,
                        button: 0,
                    },
                    EngineEvent::MouseButtonDown { button, x, y } => Self {
                        typ: tag("mousebuttondown"),
                        key: int_obj(0),
                        x,
                        y,
                        button: i32::from(button),
                    },
                    EngineEvent::MouseButtonUp { button, x, y } => Self {
                        typ: tag("mousebuttonup"),
                        key: int_obj(0),
                        x,
                        y,
                        button: i32::from(button),
                    },
                    EngineEvent::TextInput { text } => Self {
                        typ: tag("textinput"),
                        key: vm.ctx.new_str(text).into(),
                        x: 0,
                        y: 0,
                        button: 0,
                    },
                    _ => Self {
                        typ: tag("other"),
                        key: int_obj(0),
                        x: 0,
                        y: 0,
                        button: 0,
                    },
                }
            }
        }

        /// 排空事件队列（原生层 pump 每帧灌入；引擎事件 → PyEvent 映射同源）。
        #[pyfunction]
        fn get(vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let items: Vec<PyObjectRef> = bridge::drain_events()
                .into_iter()
                .map(|ev| PyEvent::from_engine(vm, ev).into_pyobject(vm))
                .collect();
            Ok(vm.ctx.new_list(items).into())
        }

        /// poll() -> Event | None：单发弹出队首（其余事件留在队列——
        /// 与 get() 的"全量排空"互补）。
        #[pyfunction]
        fn poll(vm: &VirtualMachine) -> PyResult<Option<PyObjectRef>> {
            Ok(bridge::poll_event()
                .map(|ev| PyEvent::from_engine(vm, ev).into_pyobject(vm)))
        }

        /// clear()：清空事件队列。
        #[pyfunction]
        fn clear(_vm: &VirtualMachine) {
            bridge::clear_events();
        }
    }

    #[pymodule]
    pub(crate) mod time {
        use rustpython_derive::pyclass;
        use rustpython_vm::types::Constructor;
        use rustpython_vm::{
            Py, PyPayload, PyRef, PyResult, VirtualMachine, builtins::PyType,
        };

        #[pyfunction]
        fn get_ticks(_vm: &VirtualMachine) -> u64 {
            starfish::base::time::since_start_ms()
        }

        /// pygame time.delay（Web 上 no-op——无阻塞模型，见 pygame/time.rs）。
        #[pyfunction]
        fn delay(ms: u64, _vm: &VirtualMachine) {
            starfish::pygame::time::delay(ms);
        }

        // ── Clock：帧时钟（上限节流语义——详见 pygame/time.rs）────────
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.time", name = "Clock", unhashable = true)]
        pub(crate) struct PyClock {
            pub(crate) inner: std::cell::RefCell<starfish::pygame::time::Clock>,
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
                    inner: std::cell::RefCell::new(starfish::pygame::time::Clock::new()),
                })
            }
        }
    }

    // ── font：Font pyclass（批次 D）────────────────────────────────
    #[pymodule]
    pub(crate) mod font {
        use rustpython_derive::pyclass;
        use rustpython_vm::types::Constructor;
        use starfish::pygame::font as sf_font;

        use crate::rpy::bridge;
        use rustpython_vm::builtins::{PyModule, PyStrRef, PyType};
        use rustpython_vm::{Py, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine};

        // 默认字体 = 顶层常量（见 mod pygame 顶部 DEFAULT_FONT 定义——维护者
        // 自定义入口，本模块不重复声明）
        use super::DEFAULT_FONT;

        /// pygame.font.Font——payload = starfish Font（内含字形缓存 RefCell，
        /// 单线程；无 Debug → 手写 impl）。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.font", name = "Font", unhashable = true)]
        pub(crate) struct PyFont {
            pub(crate) inner: sf_font::Font,
        }

        impl std::fmt::Debug for PyFont {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("Font(<ttf>)")
            }
        }

        /// text() 形参别名
        pub(crate) type PyFontRef = PyRef<PyFont>;

        #[pyclass(with(Constructor))]
        impl PyFont {
            /// size(text) -> (w, h)：宽度 = 字形推进和（ceil），高度 = line_height；
            /// 换行符按单行语义跳过。
            #[pymethod]
            fn size(&self, text: PyStrRef, vm: &VirtualMachine) -> PyResult<(u32, u32)> {
                let t = text
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("文本含非 UTF-8 字符"))?;
                Ok(self.inner.size(t))
            }

            #[pygetset]
            fn line_height(&self) -> u32 {
                self.inner.line_height()
            }

            /// render(text, antialias=True, color=(255,255,255)) -> Surface：
            /// 文字表面一等化（批次 F；可 blit / 可 transform）。GPU 需已
            /// init（帧循环内 ✓）。
            #[pymethod]
            fn render(
                &self,
                text: PyStrRef,
                antialias: Option<bool>,
                color: crate::rpy::args::SfColor,
                vm: &VirtualMachine,
            ) -> PyResult<super::PyRef<super::PySurface>> {
                let t = text
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("文本含非 UTF-8 字符"))?;
                let surface = self.inner.render(
                    t,
                    antialias.unwrap_or(true),
                    [color.0[0], color.0[1], color.0[2], color.0[3]],
                );
                Ok(super::PySurface { surface: std::sync::Arc::new(surface) }.into_ref(&vm.ctx))
            }
        }

        /// SysFont(name, size)：系统字体查找未实现，一律用内嵌默认字体
        /// （pygame 常用形态的占位语义）。
        #[pyfunction]
        #[allow(non_snake_case)]
        fn SysFont(_name: PyObjectRef, size: i32, vm: &VirtualMachine) -> PyResult<PyRef<PyFont>> {
            let font = font_build(None, size, vm)?;
            Ok(PyFont { inner: font }.into_ref(&vm.ctx))
        }

        /// 构造共享：None → 内嵌默认字体；str → 文件路径（wasm 无文件系统，
        /// 会报清晰错误）。
        fn font_build(
            font: Option<PyObjectRef>,
            size: i32,
            vm: &VirtualMachine,
        ) -> PyResult<sf_font::Font> {
            use rustpython_vm::convert::TryFromObject;
            let size = size.max(0) as u32;
            fn fail<E: std::fmt::Debug>(vm: &VirtualMachine, e: E) -> rustpython_vm::builtins::PyBaseExceptionRef {
                vm.new_value_error(format!("字体加载失败: {e:?}"))
            }
            // 字体缓存名 → 字节（宿主 io 预加载命中）或磁盘路径（非 wasm）
            let resolve_by_name =
                |name: &str, vm: &VirtualMachine| -> PyResult<Option<sf_font::Font>> {
                    if let Some(bytes) = bridge::font_cache_get(name) {
                        return sf_font::Font::from_bytes(bytes, size)
                            .map(Some)
                            .map_err(|e| fail(vm, e));
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if std::path::Path::new(name).is_file() {
                        return sf_font::Font::new(name, size)
                            .map(Some)
                            .map_err(|e| fail(vm, e));
                    }
                    Ok(None)
                };
            match font {
                None => sf_font::Font::from_bytes(DEFAULT_FONT.to_vec(), size)
                    .map_err(|e| fail(vm, e)),
                Some(o) => {
                    if let Some(s) = o.downcast_ref::<rustpython_vm::builtins::PyStr>() {
                        let name = s
                            .to_str()
                            .ok_or_else(|| vm.new_value_error("字体名含非 UTF-8 字符"))?;
                        match resolve_by_name(name, vm)? {
                            Some(f) => Ok(f),
                            None => Err(vm.new_value_error(format!(
                                "字体 '{name}' 未预载且文件不存在（Web 须宿主 io 预加载）"
                            ))),
                        }
                    } else if let Ok(bytes) = Vec::<u8>::try_from_object(vm, o.clone()) {
                        sf_font::Font::from_bytes(bytes, size).map_err(|e| fail(vm, e))
                    } else {
                        Err(vm.new_type_error("font 须为 None / 名字 / 路径 / bytes"))
                    }
                }
            }
        }

        impl Constructor for PyFont {
            type Args = (Option<PyObjectRef>, i32);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                vm: &VirtualMachine,
            ) -> PyResult<Self> {
                let (font, size) = args;
                Ok(Self { inner: font_build(font, size, vm)? })
            }
        }
    }

    // ── mixer：Sound pyclass + init/音量（批次 E）────────────────────
    // 混音器本体在 bridge（thread-local 保活，Drop 即停）；本模块只做
    // Python 面。Sound 构造三形态：() = 内嵌提示音（Font(None) 惯例）/
    // 路径字符串 = 文件加载 / bytes = 内存解码（OGG/MP3/FLAC/WAV 按内容探测）。
    #[pymodule]
    pub(crate) mod mixer {
        use rustpython_derive::{pymodule, pyclass, FromArgs};
        use rustpython_vm::types::Constructor;
        use rustpython_vm::builtins::PyType;
        use rustpython_vm::{Py, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine};
        use std::cell::Cell;
        use std::sync::Arc;
        use starfish::base::audio::SoundData;

        use crate::rpy::bridge;

        #[derive(Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.mixer", name = "Sound", unhashable = true)]
        pub(crate) struct PySound {
            pub(crate) data: Arc<SoundData>,
            /// 最近播放的通道（play 记录；set_volume/stop 作用于它——
            /// 通道索引停止后会被回收复用，长持需注意）
            pub(crate) channel: Cell<Option<usize>>,
        }

        /// Sound() 无参 = 内嵌提示音 / Sound("路径") / Sound(b"bytes")
        #[derive(FromArgs)]
        pub(crate) struct SoundArgs {
            #[pyarg(any, optional)]
            pub(crate) source: Option<PyObjectRef>,
        }

        impl Constructor for PySound {
            type Args = SoundArgs;

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self {
                    data: bridge::sound_source_to_data(args.source, vm)?,
                    channel: Cell::new(None),
                })
            }
        }

        /// pygame.mixer.init(channels=8)：显式初始化（不调也行——首次播放自动建）
        #[pyfunction]
        fn init(channels: Option<u32>, vm: &VirtualMachine) -> PyResult<()> {
            bridge::mixer_init(channels.unwrap_or(8), vm)
        }

        /// 卸载（Drop 即停设备）
        #[pyfunction]
        fn quit() {
            bridge::mixer_quit();
        }

        /// bool 形态（真 pygame 返回参数三元组/None——简化）
        #[pyfunction]
        fn get_init() -> bool {
            bridge::mixer_get_init()
        }

        /// 平台音频手势约束查询：Web = True（浏览器自动播放策略——
        /// AudioContext 的创建/恢复须待首次用户手势，keydown/mousedown
        /// 均算 User Activation），原生平台 = False（可直接创建播放）。
        /// 编译期 cfg 判定——供脚本自行分支，Web 的"延后触发"模式
        /// 不再强加给三平台
        #[pyfunction]
        fn needs_gesture() -> bool {
            cfg!(target_arch = "wasm32")
        }

        /// 主音量（0..1）
        #[pyfunction]
        fn set_master_volume(v: f32, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mixer(vm, |m| m.set_master_volume(v.clamp(0.0, 1.0)))
        }

        /// 任意 SFX/音乐播放中
        #[pyfunction]
        fn get_busy(vm: &VirtualMachine) -> PyResult<bool> {
            bridge::with_mixer(vm, |m| m.get_busy())
        }

        // ── pygame.mixer.music（流式 BGM；字节/文件双入口）──
        #[pymodule]
        mod music {
            use rustpython_derive::pymodule;
            use rustpython_vm::{PyResult, VirtualMachine};

            use crate::rpy::bridge;

            #[pyfunction]
            fn load(path: String, vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_load_file(&path))?;
                Ok(())
            }

            /// 字节流式加载（内嵌 BGM 零落盘；starfish music_load_bytes）
            #[pyfunction]
            fn load_bytes(bytes: Vec<u8>, vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_load_bytes(bytes))?;
                Ok(())
            }

            /// play(loops=0)：-1 无限 / 0 单次 / N 次
            #[derive(rustpython_derive::FromArgs)]
            pub(crate) struct MusicPlayArgs {
                #[pyarg(any, optional)]
                pub(crate) loops: Option<i32>,
            }

            #[pyfunction]
            fn play(args: MusicPlayArgs, vm: &VirtualMachine) -> PyResult<()> {
                // Web 手势前跳过（不排队，同 Sound.play 定约）
                if bridge::audio_gesture_locked() {
                    return Ok(());
                }
                bridge::with_mixer(vm, |m| m.music_play(args.loops.unwrap_or(0)))?;
                Ok(())
            }

            #[pyfunction]
            fn stop(vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_stop())?;
                Ok(())
            }

            #[pyfunction]
            fn pause(vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_pause())?;
                Ok(())
            }

            #[pyfunction]
            fn resume(vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_resume())?;
                Ok(())
            }

            /// 音乐音量（0..1）
            #[pyfunction]
            fn set_volume(v: f32, vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_set_volume(v.clamp(0.0, 1.0)))
            }

            /// 音乐播放中（不含 SFX——与 mixer.get_busy 分工）
            #[pyfunction]
            fn get_busy(vm: &VirtualMachine) -> PyResult<bool> {
                bridge::with_mixer(vm, |m| m.music_is_playing())
            }
        }
    }

    // ── math：模块函数直通（starfish math 现成实现）+ Vector2 基础类 ──
    // 数值入参用 ArgIntoFloat（兼收 int/float；f32 形参严格拒 int 的既有约束）。
    // Vector2 v1：构造/x/y/length/normalize/dot/加法/减法/标量乘；
    // rotate/angle_to/lerp/slerp/元素乘等留增强批。
    #[pymodule]
    pub(crate) mod math {
        use rustpython_derive::{pymodule, pyclass};
        use std::cell::Cell;
        use rustpython_vm::convert::TryFromObject;
        use rustpython_vm::function::ArgIntoFloat;
        use rustpython_vm::protocol::PyNumberMethods;
        use rustpython_vm::types::{AsNumber, Comparable, Constructor, PyComparisonOp};
        use rustpython_vm::function::PyComparisonValue;
        use rustpython_vm::{
            Py, PyObject, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine,
            builtins::{PyModule, PyStr, PyStrRef, PyType},
        };


        #[pyfunction]
        fn lerp(a: ArgIntoFloat, b: ArgIntoFloat, t: ArgIntoFloat) -> f32 {
            starfish::pygame::math::lerp(a.into_float() as f32, b.into_float() as f32, t.into_float() as f32)
        }

        #[pyfunction]
        fn clamp(x: ArgIntoFloat, min: ArgIntoFloat, max: ArgIntoFloat) -> f32 {
            starfish::pygame::math::clamp(x.into_float() as f32, min.into_float() as f32, max.into_float() as f32)
        }

        #[pyfunction]
        fn inv_lerp(a: ArgIntoFloat, b: ArgIntoFloat, v: ArgIntoFloat) -> f32 {
            starfish::pygame::math::inv_lerp(a.into_float() as f32, b.into_float() as f32, v.into_float() as f32)
        }

        #[pyfunction]
        fn remap(
            in_a: ArgIntoFloat,
            in_b: ArgIntoFloat,
            out_a: ArgIntoFloat,
            out_b: ArgIntoFloat,
            v: ArgIntoFloat,
        ) -> f32 {
            starfish::pygame::math::remap(
                in_a.into_float() as f32,
                in_b.into_float() as f32,
                out_a.into_float() as f32,
                out_b.into_float() as f32,
                v.into_float() as f32,
            )
        }

        #[pyfunction]
        fn smoothstep(e0: ArgIntoFloat, e1: ArgIntoFloat, x: ArgIntoFloat) -> f32 {
            starfish::pygame::math::smoothstep(e0.into_float() as f32, e1.into_float() as f32, x.into_float() as f32)
        }

        #[pyfunction]
        fn smootherstep(e0: ArgIntoFloat, e1: ArgIntoFloat, x: ArgIntoFloat) -> f32 {
            starfish::pygame::math::smootherstep(e0.into_float() as f32, e1.into_float() as f32, x.into_float() as f32)
        }

        // ── Vector2：基础向量（批次 H v1；rotate/angle_to 等留增强批）──
        #[derive(Clone, Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.math", name = "Vector2", unhashable = true)]
        pub(crate) struct PyVector2 {
            pub(crate) x: Cell<f32>,
            pub(crate) y: Cell<f32>,
        }

        impl PyVector2 {
            /// glam 视图（算术协议用）
            fn vec2(&self) -> starfish::pygame::math::Vector2 {
                starfish::pygame::math::Vector2::new(self.x.get(), self.y.get())
            }

            fn from_vec2(vm: &VirtualMachine, v: starfish::pygame::math::Vector2) -> PyRef<Self> {
                Self {
                    x: Cell::new(v.x),
                    y: Cell::new(v.y),
                }
                .into_ref(&vm.ctx)
            }

            // ── 算术分发（AsNumber 闭包复用）：加减仅向量；乘/除收标量
            // 或向量（元素级——glam 语义，差异标注：真 pygame 乘法仅标量）。
            // 不支持的操作数 → NotImplemented 交回数字层 → TypeError ──
            fn arith_add(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec2(vm, x.vec2() + y.vec2()).into());
                }
                Ok(vm.ctx.not_implemented())
            }

            fn arith_sub(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec2(vm, x.vec2() - y.vec2()).into());
                }
                Ok(vm.ctx.not_implemented())
            }

            fn arith_mul(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                // 全对称：任一侧向量、另一侧标量/向量（RP 无 right_* 槽——
                // 反射乘法经"尝试 b 的槽"以同参数序到达，必须双侧处理）
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec2(vm, x.vec2() * y.vec2()).into());
                }
                if let (Some(x), Ok(k)) = (
                    a.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, b.to_owned()),
                ) {
                    return Ok(Self::from_vec2(vm, x.vec2() * (k.into_float() as f32)).into());
                }
                if let (Some(y), Ok(k)) = (
                    b.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, a.to_owned()),
                ) {
                    return Ok(Self::from_vec2(vm, y.vec2() * (k.into_float() as f32)).into());
                }
                Ok(vm.ctx.not_implemented())
            }

            fn arith_true_div(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec2(vm, x.vec2() / y.vec2()).into());
                }
                if let (Some(x), Ok(k)) = (
                    a.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, b.to_owned()),
                ) {
                    let k = k.into_float() as f32;
                    if k.abs() < f32::EPSILON {
                        return Err(vm.new_zero_division_error("vector division by zero"));
                    }
                    return Ok(Self::from_vec2(vm, x.vec2() / k).into());
                }
                Ok(vm.ctx.not_implemented())
            }
        }

        impl AsNumber for PyVector2 {
            fn as_number() -> &'static PyNumberMethods {
                static AS_NUMBER: PyNumberMethods = PyNumberMethods {
                    add: Some(PyVector2::arith_add),
                    subtract: Some(PyVector2::arith_sub),
                    multiply: Some(PyVector2::arith_mul),
                    true_divide: Some(PyVector2::arith_true_div),
                    // v += w / v -= w：就地变更（恒等保持——pygame 向量
                    // 语义）；标量操作数 → NotImplemented 回退重绑定
                    inplace_add: Some(|a, b, vm| {
                        if let (Some(zelf), Some(y)) =
                            (a.downcast_ref::<PyVector2>(), b.downcast_ref::<PyVector2>())
                        {
                            let r = zelf.vec2() + y.vec2();
                            zelf.x.set(r.x);
                            zelf.y.set(r.y);
                            return Ok(a.to_owned());
                        }
                        Ok(vm.ctx.not_implemented())
                    }),
                    inplace_subtract: Some(|a, b, vm| {
                        if let (Some(zelf), Some(y)) =
                            (a.downcast_ref::<PyVector2>(), b.downcast_ref::<PyVector2>())
                        {
                            let r = zelf.vec2() - y.vec2();
                            zelf.x.set(r.x);
                            zelf.y.set(r.y);
                            return Ok(a.to_owned());
                        }
                        Ok(vm.ctx.not_implemented())
                    }),
                    negative: Some(|number, vm| {
                        let v = PyVector2::number_downcast(number).vec2();
                        Ok(PyVector2::from_vec2(vm, -v).into())
                    }),
                    ..PyNumberMethods::NOT_IMPLEMENTED
                };
                &AS_NUMBER
            }
        }

        #[pyclass(with(Comparable, Constructor, AsNumber))]
        impl PyVector2 {
            // x/y 属性（读写；pygame Vector2 的基础属性面）
            #[pygetset]
            fn x(&self) -> f32 {
                self.x.get()
            }
            #[pygetset(setter)]
            fn set_x(&self, v: f32) {
                self.x.set(v);
            }
            #[pygetset]
            fn y(&self) -> f32 {
                self.y.get()
            }
            #[pygetset(setter)]
            fn set_y(&self, v: f32) {
                self.y.set(v);
            }

            #[pymethod]
            fn dot(&self, other: PyRef<Self>) -> f32 {
                self.x.get() * other.x.get() + self.y.get() * other.y.get()
            }

            #[pymethod]
            fn length(&self) -> f32 {
                (self.x.get() * self.x.get() + self.y.get() * self.y.get()).sqrt()
            }

            #[pymethod]
            fn normalize(&self, vm: &VirtualMachine) -> PyResult<PyRef<Self>> {
                let len = self.length();
                let (x, y) = if len > f32::EPSILON {
                    (self.x.get() / len, self.y.get() / len)
                } else {
                    (0.0, 0.0) // 零向量归一化 = 零向量（pygame 语义）
                };
                Ok(PyVector2 {
                    x: Cell::new(x),
                    y: Cell::new(y),
                }
                .into_ref(&vm.ctx))
            }

        }

        impl Comparable for PyVector2 {
            fn cmp(
                zelf: &Py<Self>,
                other: &PyObject,
                op: PyComparisonOp,
                _vm: &VirtualMachine,
            ) -> PyResult<PyComparisonValue> {
                if op == PyComparisonOp::Eq {
                    if let Some(o) = other.downcast_ref::<PyVector2>() {
                        return Ok(PyComparisonValue::Implemented(
                            zelf.x.get() == o.x.get() && zelf.y.get() == o.y.get(),
                        ));
                    }
                }
                Ok(PyComparisonValue::NotImplemented)
            }
        }

        impl Constructor for PyVector2 {
            type Args = (ArgIntoFloat, ArgIntoFloat);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                _vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self {
                    x: Cell::new(args.0.into_float() as f32),
                    y: Cell::new(args.1.into_float() as f32),
                })
            }
        }

        // ── Vector3：三维向量（批次 K；starfish math Vector3 = glam::Vec3）──
        #[derive(Clone, Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.math", name = "Vector3", unhashable = true)]
        pub(crate) struct PyVector3 {
            pub(crate) x: Cell<f32>,
            pub(crate) y: Cell<f32>,
            pub(crate) z: Cell<f32>,
        }

        impl PyVector3 {
            /// glam 视图（方法面从此构造）
            fn vec(&self) -> starfish::pygame::math::Vector3 {
                starfish::pygame::math::Vector3::new(self.x.get(), self.y.get(), self.z.get())
            }

            fn from_vec(vm: &VirtualMachine, v: starfish::pygame::math::Vector3) -> PyRef<Self> {
                Self {
                    x: Cell::new(v.x),
                    y: Cell::new(v.y),
                    z: Cell::new(v.z),
                }
                .into_ref(&vm.ctx)
            }

            // ── 算术分发（AsNumber 闭包复用）：同 Vector2 约定 ──
            fn arith_add(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec(vm, x.vec() + y.vec()).into());
                }
                Ok(vm.ctx.not_implemented())
            }

            fn arith_sub(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec(vm, x.vec() - y.vec()).into());
                }
                Ok(vm.ctx.not_implemented())
            }

            fn arith_mul(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                // 全对称：任一侧向量、另一侧标量/向量（同 Vector2 注）
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec(vm, x.vec() * y.vec()).into());
                }
                if let (Some(x), Ok(k)) = (
                    a.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, b.to_owned()),
                ) {
                    return Ok(Self::from_vec(vm, x.vec() * (k.into_float() as f32)).into());
                }
                if let (Some(y), Ok(k)) = (
                    b.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, a.to_owned()),
                ) {
                    return Ok(Self::from_vec(vm, y.vec() * (k.into_float() as f32)).into());
                }
                Ok(vm.ctx.not_implemented())
            }

            fn arith_true_div(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self::from_vec(vm, x.vec() / y.vec()).into());
                }
                if let (Some(x), Ok(k)) = (
                    a.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, b.to_owned()),
                ) {
                    let k = k.into_float() as f32;
                    if k.abs() < f32::EPSILON {
                        return Err(vm.new_zero_division_error("vector division by zero"));
                    }
                    return Ok(Self::from_vec(vm, x.vec() / k).into());
                }
                Ok(vm.ctx.not_implemented())
            }
        }

        impl AsNumber for PyVector3 {
            fn as_number() -> &'static PyNumberMethods {
                static AS_NUMBER: PyNumberMethods = PyNumberMethods {
                    add: Some(PyVector3::arith_add),
                    subtract: Some(PyVector3::arith_sub),
                    multiply: Some(PyVector3::arith_mul),
                    true_divide: Some(PyVector3::arith_true_div),
                    inplace_add: Some(|a, b, vm| {
                        if let (Some(zelf), Some(y)) =
                            (a.downcast_ref::<PyVector3>(), b.downcast_ref::<PyVector3>())
                        {
                            let r = zelf.vec() + y.vec();
                            zelf.x.set(r.x);
                            zelf.y.set(r.y);
                            zelf.z.set(r.z);
                            return Ok(a.to_owned());
                        }
                        Ok(vm.ctx.not_implemented())
                    }),
                    inplace_subtract: Some(|a, b, vm| {
                        if let (Some(zelf), Some(y)) =
                            (a.downcast_ref::<PyVector3>(), b.downcast_ref::<PyVector3>())
                        {
                            let r = zelf.vec() - y.vec();
                            zelf.x.set(r.x);
                            zelf.y.set(r.y);
                            zelf.z.set(r.z);
                            return Ok(a.to_owned());
                        }
                        Ok(vm.ctx.not_implemented())
                    }),
                    negative: Some(|number, vm| {
                        let v = PyVector3::number_downcast(number).vec();
                        Ok(PyVector3::from_vec(vm, -v).into())
                    }),
                    ..PyNumberMethods::NOT_IMPLEMENTED
                };
                &AS_NUMBER
            }
        }

        #[pyclass(with(Comparable, Constructor, AsNumber))]
        impl PyVector3 {
            #[pygetset]
            fn x(&self) -> f32 {
                self.x.get()
            }
            #[pygetset(setter)]
            fn set_x(&self, v: f32) {
                self.x.set(v);
            }
            #[pygetset]
            fn y(&self) -> f32 {
                self.y.get()
            }
            #[pygetset(setter)]
            fn set_y(&self, v: f32) {
                self.y.set(v);
            }
            #[pygetset]
            fn z(&self) -> f32 {
                self.z.get()
            }
            #[pygetset(setter)]
            fn set_z(&self, v: f32) {
                self.z.set(v);
            }

            #[pymethod]
            fn dot(&self, other: PyRef<Self>) -> f32 {
                self.vec().dot(other.vec())
            }

            /// 叉积（右手系）
            #[pymethod]
            fn cross(&self, other: PyRef<Self>, vm: &VirtualMachine) -> PyRef<Self> {
                Self::from_vec(vm, self.vec().cross(other.vec()))
            }

            #[pymethod]
            fn length(&self) -> f32 {
                self.vec().length()
            }

            #[pymethod]
            fn length_squared(&self) -> f32 {
                self.vec().length_squared()
            }

            /// 归一化（返回新向量；零向量 = 零向量，pygame 语义）
            #[pymethod]
            fn normalize(&self, vm: &VirtualMachine) -> PyRef<Self> {
                let v = self.vec();
                if v.length_squared() > f32::EPSILON {
                    Self::from_vec(vm, v.normalize())
                } else {
                    Self::from_vec(vm, starfish::pygame::math::Vector3::ZERO)
                }
            }

            /// 到 other 的直线距离（pygame 命名）。
            #[pymethod]
            fn distance_to(&self, other: PyRef<Self>) -> f32 {
                self.vec().distance(other.vec())
            }

            /// 线性插值（t = 0 本体 → 1 other）。
            #[pymethod]
            fn lerp(&self, other: PyRef<Self>, t: ArgIntoFloat, vm: &VirtualMachine) -> PyRef<Self> {
                Self::from_vec(vm, self.vec().lerp(other.vec(), t.into_float() as f32))
            }

            /// 就地缩放到指定长度（pygame 语义：改自身）。
            #[pymethod]
            fn scale_to_length(&self, target: ArgIntoFloat) {
                let v = self.vec();
                let len = v.length();
                if len > f32::EPSILON {
                    let k = target.into_float() as f32 / len;
                    self.x.set(v.x * k);
                    self.y.set(v.y * k);
                    self.z.set(v.z * k);
                }
            }

        }

        impl Comparable for PyVector3 {
            fn cmp(
                zelf: &Py<Self>,
                other: &PyObject,
                op: PyComparisonOp,
                _vm: &VirtualMachine,
            ) -> PyResult<PyComparisonValue> {
                if op == PyComparisonOp::Eq {
                    if let Some(o) = other.downcast_ref::<PyVector3>() {
                        return Ok(PyComparisonValue::Implemented(
                            zelf.x.get() == o.x.get()
                                && zelf.y.get() == o.y.get()
                                && zelf.z.get() == o.z.get(),
                        ));
                    }
                }
                Ok(PyComparisonValue::NotImplemented)
            }
        }

        impl Constructor for PyVector3 {
            type Args = (ArgIntoFloat, ArgIntoFloat, ArgIntoFloat);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                _vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self {
                    x: Cell::new(args.0.into_float() as f32),
                    y: Cell::new(args.1.into_float() as f32),
                    z: Cell::new(args.2.into_float() as f32),
                })
            }
        }

        // ── Quaternion：四元数（starfish 扩展类型 = glam::Quat 直通）──
        // 构造经模块函数（quat_identity / quat_from_axis_angle /
        // quat_from_euler，度数制）；类直呼 = xyzw 四参。
        #[derive(Clone, Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.math", name = "Quaternion", unhashable = true)]
        pub(crate) struct PyQuaternion(pub(crate) Cell<starfish::pygame::math::Quaternion>);

        impl PyQuaternion {
            /// 算术分发（批次 P）：quat × quat（组合）或 quat × 标量
            ///（缩放，glam 语义）；全对称（反射经 b 的槽同参数序到达）。
            fn arith_mul(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self(Cell::new(x.0.get() * y.0.get()))
                        .into_ref(&vm.ctx)
                        .into());
                }
                if let (Some(x), Ok(k)) = (
                    a.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, b.to_owned()),
                ) {
                    return Ok(Self(Cell::new(x.0.get() * (k.into_float() as f32)))
                        .into_ref(&vm.ctx)
                        .into());
                }
                if let (Some(y), Ok(k)) = (
                    b.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, a.to_owned()),
                ) {
                    return Ok(Self(Cell::new(y.0.get() * (k.into_float() as f32)))
                        .into_ref(&vm.ctx)
                        .into());
                }
                Ok(vm.ctx.not_implemented())
            }
        }

        impl AsNumber for PyQuaternion {
            fn as_number() -> &'static PyNumberMethods {
                static AS_NUMBER: PyNumberMethods = PyNumberMethods {
                    multiply: Some(PyQuaternion::arith_mul),
                    negative: Some(|number, vm| {
                        let q = PyQuaternion::number_downcast(number).0.get();
                        Ok(PyQuaternion(Cell::new(-q)).into_ref(&vm.ctx).into())
                    }),
                    ..PyNumberMethods::NOT_IMPLEMENTED
                };
                &AS_NUMBER
            }
        }

        #[pyclass(with(Comparable, Constructor, AsNumber))]
        impl PyQuaternion {
            #[pygetset]
            fn x(&self) -> f32 {
                self.0.get().x
            }
            #[pygetset]
            fn y(&self) -> f32 {
                self.0.get().y
            }
            #[pygetset]
            fn z(&self) -> f32 {
                self.0.get().z
            }
            #[pygetset]
            fn w(&self) -> f32 {
                self.0.get().w
            }

            #[pymethod]
            fn length(&self) -> f32 {
                self.0.get().length()
            }

            /// 归一化（返回新四元数；零长度 = 原样——glam 归零会产生 NaN）
            #[pymethod]
            fn normalize(&self, vm: &VirtualMachine) -> PyRef<Self> {
                let q = self.0.get();
                let out = if q.length_squared() > f32::EPSILON {
                    q.normalize()
                } else {
                    q
                };
                Self(Cell::new(out)).into_ref(&vm.ctx)
            }

            #[pymethod]
            fn dot(&self, other: PyRef<Self>) -> f32 {
                self.0.get().dot(other.0.get())
            }

            /// 球面插值（t = 0 本体 → 1 other）。
            #[pymethod]
            fn slerp(&self, other: PyRef<Self>, t: ArgIntoFloat, vm: &VirtualMachine) -> PyRef<Self> {
                Self(Cell::new(
                    self.0.get().slerp(other.0.get(), t.into_float() as f32),
                ))
                .into_ref(&vm.ctx)
            }
        }

        impl Comparable for PyQuaternion {
            fn cmp(
                zelf: &Py<Self>,
                other: &PyObject,
                op: PyComparisonOp,
                _vm: &VirtualMachine,
            ) -> PyResult<PyComparisonValue> {
                if op == PyComparisonOp::Eq {
                    if let Some(o) = other.downcast_ref::<PyQuaternion>() {
                        let (a, b) = (zelf.0.get(), o.0.get());
                        return Ok(PyComparisonValue::Implemented(
                            a.x == b.x && a.y == b.y && a.z == b.z && a.w == b.w,
                        ));
                    }
                }
                Ok(PyComparisonValue::NotImplemented)
            }
        }

        impl Constructor for PyQuaternion {
            type Args = (ArgIntoFloat, ArgIntoFloat, ArgIntoFloat, ArgIntoFloat);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                _vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self(Cell::new(starfish::pygame::math::Quaternion::from_xyzw(
                    args.0.into_float() as f32,
                    args.1.into_float() as f32,
                    args.2.into_float() as f32,
                    args.3.into_float() as f32,
                ))))
            }
        }

        // ── Mat4：4x4 矩阵（glam 直通；无算符协议——乘法用 mul_mat4）──
        // 构造经模块函数（mat4_identity / mat4_from_quat / mat4_translation /
        // mat4_scale）；直呼不可实例化。
        #[derive(Clone, Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.math", name = "Mat4", unhashable = true)]
        pub(crate) struct PyMat4(pub(crate) Cell<starfish::pygame::math::Mat4>);

        impl PyMat4 {
            /// 算术分发（批次 P）：mat × mat（组合）或 mat × 标量；全对称。
            fn arith_mul(a: &PyObject, b: &PyObject, vm: &VirtualMachine) -> PyResult {
                if let (Some(x), Some(y)) =
                    (a.downcast_ref::<Self>(), b.downcast_ref::<Self>())
                {
                    return Ok(Self(Cell::new(x.0.get() * y.0.get()))
                        .into_ref(&vm.ctx)
                        .into());
                }
                if let (Some(x), Ok(k)) = (
                    a.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, b.to_owned()),
                ) {
                    return Ok(Self(Cell::new(x.0.get() * (k.into_float() as f32)))
                        .into_ref(&vm.ctx)
                        .into());
                }
                if let (Some(y), Ok(k)) = (
                    b.downcast_ref::<Self>(),
                    ArgIntoFloat::try_from_object(vm, a.to_owned()),
                ) {
                    return Ok(Self(Cell::new(y.0.get() * (k.into_float() as f32)))
                        .into_ref(&vm.ctx)
                        .into());
                }
                Ok(vm.ctx.not_implemented())
            }
        }

        impl AsNumber for PyMat4 {
            fn as_number() -> &'static PyNumberMethods {
                static AS_NUMBER: PyNumberMethods = PyNumberMethods {
                    multiply: Some(PyMat4::arith_mul),
                    negative: Some(|number, vm| {
                        let m = PyMat4::number_downcast(number).0.get();
                        Ok(PyMat4(Cell::new(-m)).into_ref(&vm.ctx).into())
                    }),
                    ..PyNumberMethods::NOT_IMPLEMENTED
                };
                &AS_NUMBER
            }
        }

        #[pyclass(with(AsNumber))]
        impl PyMat4 {
            /// 矩阵乘（self * other——变换叠加顺序同 glam）。
            #[pymethod]
            fn mul_mat4(&self, other: PyRef<Self>, vm: &VirtualMachine) -> PyRef<Self> {
                Self(Cell::new(self.0.get() * other.0.get())).into_ref(&vm.ctx)
            }

            /// 变换三维点（含透视除法前的位置变换；glam transform_point3）。
            #[pymethod]
            fn transform_point3(&self, v: PyRef<PyVector3>, vm: &VirtualMachine) -> PyRef<PyVector3> {
                PyVector3::from_vec(vm, self.0.get().transform_point3(v.vec()))
            }
        }

        // ── 3D 数学模块函数（度数制入口；弧度制走 glam 语义层）─────────

        /// 单位四元数。
        #[pyfunction]
        fn quat_identity(vm: &VirtualMachine) -> PyRef<PyQuaternion> {
            PyQuaternion(Cell::new(starfish::pygame::math::Quaternion::IDENTITY))
                .into_ref(&vm.ctx)
        }

        /// 轴角 → 四元数（axis 分量 + 度数制角；轴无需预归一）。
        #[pyfunction]
        fn quat_from_axis_angle(
            ax: ArgIntoFloat,
            ay: ArgIntoFloat,
            az: ArgIntoFloat,
            deg: ArgIntoFloat,
            vm: &VirtualMachine,
        ) -> PyRef<PyQuaternion> {
            let axis = starfish::pygame::math::Vector3::new(
                ax.into_float() as f32,
                ay.into_float() as f32,
                az.into_float() as f32,
            )
            .normalize_or_zero();
            let q = starfish::pygame::math::Quaternion::from_axis_angle(
                axis,
                (deg.into_float() as f32).to_radians(),
            );
            PyQuaternion(Cell::new(q)).into_ref(&vm.ctx)
        }

        /// 欧拉角 → 四元数（pitch/yaw/roll 度数制，XYZ 序——glam EulerRot::XYZ）。
        #[pyfunction]
        fn quat_from_euler(
            pitch: ArgIntoFloat,
            yaw: ArgIntoFloat,
            roll: ArgIntoFloat,
            vm: &VirtualMachine,
        ) -> PyRef<PyQuaternion> {
            use glam::EulerRot;
            let q = starfish::pygame::math::Quaternion::from_euler(
                EulerRot::XYZ,
                (pitch.into_float() as f32).to_radians(),
                (yaw.into_float() as f32).to_radians(),
                (roll.into_float() as f32).to_radians(),
            );
            PyQuaternion(Cell::new(q)).into_ref(&vm.ctx)
        }

        /// 4x4 单位矩阵。
        #[pyfunction]
        fn mat4_identity(vm: &VirtualMachine) -> PyRef<PyMat4> {
            PyMat4(Cell::new(starfish::pygame::math::Mat4::IDENTITY)).into_ref(&vm.ctx)
        }

        /// 四元数 → 旋转矩阵。
        #[pyfunction]
        fn mat4_from_quat(q: PyRef<PyQuaternion>, vm: &VirtualMachine) -> PyRef<PyMat4> {
            PyMat4(Cell::new(
                starfish::pygame::math::Mat4::from_quat(q.0.get()),
            ))
            .into_ref(&vm.ctx)
        }

        /// 平移矩阵。
        #[pyfunction]
        fn mat4_translation(
            x: ArgIntoFloat,
            y: ArgIntoFloat,
            z: ArgIntoFloat,
            vm: &VirtualMachine,
        ) -> PyRef<PyMat4> {
            PyMat4(Cell::new(starfish::pygame::math::Mat4::from_translation(
                starfish::pygame::math::Vector3::new(
                    x.into_float() as f32,
                    y.into_float() as f32,
                    z.into_float() as f32,
                ),
            )))
            .into_ref(&vm.ctx)
        }

        /// 缩放矩阵。
        #[pyfunction]
        fn mat4_scale(
            x: ArgIntoFloat,
            y: ArgIntoFloat,
            z: ArgIntoFloat,
            vm: &VirtualMachine,
        ) -> PyRef<PyMat4> {
            PyMat4(Cell::new(starfish::pygame::math::Mat4::from_scale(
                starfish::pygame::math::Vector3::new(
                    x.into_float() as f32,
                    y.into_float() as f32,
                    z.into_float() as f32,
                ),
            )))
            .into_ref(&vm.ctx)
        }
    }

    // ── sndarray：Sound ↔ 嵌套 list 互转（Web/无 numpy 环境用 list 承载）──
    #[pymodule]
    pub(crate) mod sndarray {
        use rustpython_derive::pymodule;
        use rustpython_vm::convert::TryFromObject;
        use rustpython_vm::{PyObjectRef, PyRef, PyResult, VirtualMachine};

        use super::mixer::PySound;

        /// array(sound) -> [[L, R], ...]（立体声）/ [[v], ...]（单声道）——
        /// 无 numpy 环境用嵌套 list 承载（真 pygame 是 numpy 数组，差异标注）
        #[pyfunction]
        fn array(sound: PyRef<PySound>, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let data = sound.data.clone();
            let rows: Vec<PyObjectRef> = match &data.channels {
                starfish::base::audio::AudioChannels::Mono(samples) => samples
                    .iter()
                    .map(|v| vm.new_tuple((*v,)).into())
                    .collect(),
                starfish::base::audio::AudioChannels::Stereo(frames) => frames
                    .iter()
                    .map(|f| vm.new_tuple((f.left, f.right)).into())
                    .collect(),
            };
            Ok(vm.ctx.new_list(rows).into())
        }

        /// make_sound(嵌套list) -> Sound：行宽 1 = 单声道 / 2 = 立体声（可混排
        /// 以先出现的行宽为准；采样率 v1 固定 44100）
        #[pyfunction]
        fn make_sound(array: PyObjectRef, vm: &VirtualMachine) -> PyResult<PySound> {
            use rustpython_vm::convert::TryFromObject;
            let rows = vm.extract_elements_with(&array, |o| {
                vm.extract_elements_with(&o, |v| f32::try_from_object(vm, v))
            })?;
            let mut flat: Vec<f32> = Vec::new();
            let mut channels = 0usize;
            for row in &rows {
                match row.len() {
                    1 => {
                        if channels == 0 {
                            channels = 1;
                        }
                        flat.push(row[0]);
                    }
                    2 => {
                        if channels == 0 {
                            channels = 2;
                        }
                        if channels != 2 {
                            return Err(vm.new_value_error("行宽不一致（单声道与立体声混排）"));
                        }
                        flat.push(row[0]);
                        flat.push(row[1]);
                    }
                    _ => {
                        return Err(vm.new_value_error(format!(
                            "行宽须为 1（单声道）或 2（立体声），得到 {}",
                            row.len()
                        )))
                    }
                }
            }
            let rate = 44_100u32; // v1 固定（真 pygame 从 array 推断采样率的形态留 v2）
            let data = match channels {
                1 => starfish::base::audio::SoundData::from_mono_f32(&flat, rate),
                _ => starfish::base::audio::SoundData::from_interleaved_f32(&flat, rate),
            };
            Ok(PySound {
                data: std::sync::Arc::new(data),
                channel: std::cell::Cell::new(None),
            })
        }
    }

    // ── version：薄模块直通 ──
    #[pymodule]
    pub(crate) mod version {
        use rustpython_vm::VirtualMachine;

        /// pygame.version.ver：starfish pygame 层版本（= crate 版本）
        #[pyattr(name = "ver")]
        const VER: &str = starfish::pygame::version::VERSION;

        /// pygame.version.vernum：版本三元组
        #[pyfunction]
        fn get_version() -> (u8, u8, u8) {
            starfish::pygame::version::get_version()
        }
    }

    // ── transform：GPU 采样变换（批次 F；须 display init 后）─────────
    #[pymodule]
    pub(crate) mod transform {
        use rustpython_derive::pymodule;
        use rustpython_vm::function::ArgIntoFloat;
        use rustpython_vm::{PyPayload, PyRef, PyResult, VirtualMachine};
        use starfish::pygame::transform as sf_transform;

        use super::PySurface;

        /// 角度/倍率入参：ArgIntoFloat 兼收 int/float（RP f32 形参严格拒 int）
        fn f32_of(v: ArgIntoFloat) -> f32 {
            v.into_float() as f32
        }

        #[pyfunction]
        fn flip_x(s: PyRef<PySurface>, vm: &VirtualMachine) -> PyResult<PyRef<PySurface>> {
            Ok(PySurface { surface: std::sync::Arc::new(sf_transform::flip_x(&s.surface)) }.into_ref(&vm.ctx))
        }

        #[pyfunction]
        fn flip_y(s: PyRef<PySurface>, vm: &VirtualMachine) -> PyResult<PyRef<PySurface>> {
            Ok(PySurface { surface: std::sync::Arc::new(sf_transform::flip_y(&s.surface)) }.into_ref(&vm.ctx))
        }

        /// scale(size)：线性采样缩放（恒平滑，无 smoothscale 之分）
        #[pyfunction]
        fn scale(s: PyRef<PySurface>, size: crate::rpy::args::SfPoint, vm: &VirtualMachine) -> PyResult<PyRef<PySurface>> {
            Ok(PySurface {
                surface: std::sync::Arc::new(sf_transform::scale(
                    &s.surface,
                    (size.0.max(0) as u32, size.1.max(0) as u32),
                )),
            }
            .into_ref(&vm.ctx))
        }

        /// smoothscale(size) -> Surface：平滑缩放。本层采样器恒线性
        /// （starfish transform 模块定约：smoothscale 与 scale 等同，
        /// 像素风 nearest 变体位 v2 预留）——真 pygame 两名分
        /// nearest/linear，差异标注
        #[pyfunction]
        fn smoothscale(s: PyRef<PySurface>, size: crate::rpy::args::SfPoint, vm: &VirtualMachine) -> PyResult<PyRef<PySurface>> {
            Ok(PySurface {
                surface: std::sync::Arc::new(sf_transform::scale(
                    &s.surface,
                    (size.0.max(0) as u32, size.1.max(0) as u32),
                )),
            }
            .into_ref(&vm.ctx))
        }

        /// rotate(deg)：逆时针度数，输出包围盒，内容居中
        #[pyfunction]
        fn rotate(s: PyRef<PySurface>, deg: ArgIntoFloat, vm: &VirtualMachine) -> PyResult<PyRef<PySurface>> {
            Ok(PySurface { surface: std::sync::Arc::new(sf_transform::rotate(&s.surface, f32_of(deg))) }.into_ref(&vm.ctx))
        }

        #[pyfunction]
        fn rotozoom(
            s: PyRef<PySurface>,
            deg: ArgIntoFloat,
            zoom: ArgIntoFloat,
            vm: &VirtualMachine,
        ) -> PyResult<PyRef<PySurface>> {
            Ok(PySurface {
                surface: std::sync::Arc::new(sf_transform::rotozoom(
                    &s.surface,
                    f32_of(deg),
                    f32_of(zoom),
                )),
            }
            .into_ref(&vm.ctx))
        }
    }

    // ── image：加载 / 保存（批次 F）────────────────────────────────
    // save 本质 = GPU 回读（CPU 落地），仅桌面同步形态；wasm 出栈至
    // save_async 异步语义批次。
    #[pymodule]
    pub(crate) mod image {
        use rustpython_derive::pymodule;
        use rustpython_vm::{PyPayload, PyRef, PyResult, VirtualMachine};
        use starfish::pygame::image as sf_image;

        use super::PySurface;

        #[pyfunction]
        fn get_extended() -> bool {
            sf_image::get_extended()
        }

        /// 桌面：路径加载（std::fs）。Web 无文件系统——用 load_from_bytes。
        #[cfg(not(target_arch = "wasm32"))]
        #[pyfunction]
        fn load(path: String, vm: &VirtualMachine) -> PyResult<PyRef<PySurface>> {
            match sf_image::load(&path) {
                Ok(s) => Ok(PySurface { surface: std::sync::Arc::new(s) }.into_ref(&vm.ctx)),
                Err(e) => Err(vm.new_value_error(format!("图片加载失败: {e:?}"))),
            }
        }

        /// Web：路径加载不可用（无文件系统），显式报错指路 load_from_bytes
        #[cfg(target_arch = "wasm32")]
        #[pyfunction]
        fn load(_path: String, vm: &VirtualMachine) -> PyResult<PyRef<PySurface>> {
            Err(vm.new_value_error("Web 无文件系统——用 image.load_from_bytes(bytes)"))
        }

        /// PNG/JPEG/BMP/GIF/WebP 按内容解码（Web 首选入口）
        #[pyfunction]
        fn load_from_bytes(bytes: Vec<u8>, vm: &VirtualMachine) -> PyResult<PyRef<PySurface>> {
            match sf_image::load_from_bytes(&bytes) {
                Ok(s) => Ok(PySurface { surface: std::sync::Arc::new(s) }.into_ref(&vm.ctx)),
                Err(e) => Err(vm.new_value_error(format!("图片解码失败: {e:?}"))),
            }
        }

        /// 桌面专用（block_on 回读落盘；格式按扩展名，无扩展名默认 PNG）
        #[cfg(not(target_arch = "wasm32"))]
        #[pyfunction]
        fn save(surf: PyRef<PySurface>, path: String, vm: &VirtualMachine) -> PyResult<()> {
            sf_image::save(&surf.surface, &path)
                .map_err(|e| vm.new_value_error(format!("图片保存失败: {e:?}")))
        }

        /// Web：save 须 save_async 异步语义（批次出栈）
        #[cfg(target_arch = "wasm32")]
        #[pyfunction]
        fn save(_surf: PyRef<PySurface>, _path: String, vm: &VirtualMachine) -> PyResult<()> {
            Err(vm.new_not_implemented_error("image.save 桌面专用（Web 须 save_async 异步语义，后续批次）"))
        }
    }

    // ── key：按下视图查询（K_* 常量在 pygame 顶层，见 KEY_TABLE）─────
    // 按下状态由 starfish 事件泵喂（bridge::pump → event::get 自动驱动）。
    #[pymodule]
    pub(crate) mod key {
        use rustpython_vm::{Py, PyPayload, PyRef, PyResult, VirtualMachine, builtins::PyModule, pyclass};
        use starfish::base::window::KeyCode;
        use crate::rpy::native_pygame::pygame::event::PyKey;

        /// K_* 全表（starfish locals::key 的 108 键，typed KeyCode；
        /// SHIFT/CTRL/ALT/META 为左键别名）。exec 时注册为 pygame 顶层常量
        /// （真 pygame 惯例：K_* 在顶层而非 key 子模块）。
        /// 由于依赖PyKey类型，所以locals模块必须在event后面。
        pub(crate) const KEY_TABLE: &[(&str, KeyCode)] = &[
            // 字母
            ("K_a", KeyCode::KeyA), ("K_b", KeyCode::KeyB), ("K_c", KeyCode::KeyC),
            ("K_d", KeyCode::KeyD), ("K_e", KeyCode::KeyE), ("K_f", KeyCode::KeyF),
            ("K_g", KeyCode::KeyG), ("K_h", KeyCode::KeyH), ("K_i", KeyCode::KeyI),
            ("K_j", KeyCode::KeyJ), ("K_k", KeyCode::KeyK), ("K_l", KeyCode::KeyL),
            ("K_m", KeyCode::KeyM), ("K_n", KeyCode::KeyN), ("K_o", KeyCode::KeyO),
            ("K_p", KeyCode::KeyP), ("K_q", KeyCode::KeyQ), ("K_r", KeyCode::KeyR),
            ("K_s", KeyCode::KeyS), ("K_t", KeyCode::KeyT), ("K_u", KeyCode::KeyU),
            ("K_v", KeyCode::KeyV), ("K_w", KeyCode::KeyW), ("K_x", KeyCode::KeyX),
            ("K_y", KeyCode::KeyY), ("K_z", KeyCode::KeyZ),
            // 数字行
            ("K_0", KeyCode::Digit0), ("K_1", KeyCode::Digit1), ("K_2", KeyCode::Digit2),
            ("K_3", KeyCode::Digit3), ("K_4", KeyCode::Digit4), ("K_5", KeyCode::Digit5),
            ("K_6", KeyCode::Digit6), ("K_7", KeyCode::Digit7), ("K_8", KeyCode::Digit8),
            ("K_9", KeyCode::Digit9),
            // 功能键
            ("K_F1", KeyCode::F1), ("K_F2", KeyCode::F2), ("K_F3", KeyCode::F3),
            ("K_F4", KeyCode::F4), ("K_F5", KeyCode::F5), ("K_F6", KeyCode::F6),
            ("K_F7", KeyCode::F7), ("K_F8", KeyCode::F8), ("K_F9", KeyCode::F9),
            ("K_F10", KeyCode::F10), ("K_F11", KeyCode::F11), ("K_F12", KeyCode::F12),
            // 控制键
            ("K_ESCAPE", KeyCode::Escape), ("K_TAB", KeyCode::Tab),
            ("K_CAPSLOCK", KeyCode::CapsLock), ("K_SPACE", KeyCode::Space),
            ("K_RETURN", KeyCode::Enter), ("K_BACKSPACE", KeyCode::Backspace),
            ("K_DELETE", KeyCode::Delete), ("K_INSERT", KeyCode::Insert),
            // 修饰键（左键别名组）
            ("K_SHIFT", KeyCode::ShiftLeft), ("K_LSHIFT", KeyCode::ShiftLeft),
            ("K_RSHIFT", KeyCode::ShiftRight),
            ("K_CTRL", KeyCode::ControlLeft), ("K_LCTRL", KeyCode::ControlLeft),
            ("K_RCTRL", KeyCode::ControlRight),
            ("K_ALT", KeyCode::AltLeft), ("K_LALT", KeyCode::AltLeft),
            ("K_RALT", KeyCode::AltRight),
            ("K_META", KeyCode::MetaLeft), ("K_LMETA", KeyCode::MetaLeft),
            ("K_RMETA", KeyCode::MetaRight),
            ("K_MENU", KeyCode::ContextMenu),
            // 方向 / 翻页
            ("K_UP", KeyCode::ArrowUp), ("K_DOWN", KeyCode::ArrowDown),
            ("K_LEFT", KeyCode::ArrowLeft), ("K_RIGHT", KeyCode::ArrowRight),
            ("K_HOME", KeyCode::Home), ("K_END", KeyCode::End),
            ("K_PAGEUP", KeyCode::PageUp), ("K_PAGEDOWN", KeyCode::PageDown),
            // 符号
            ("K_MINUS", KeyCode::Minus), ("K_EQUALS", KeyCode::Equal),
            ("K_LEFTBRACKET", KeyCode::BracketLeft), ("K_RIGHTBRACKET", KeyCode::BracketRight),
            ("K_BACKSLASH", KeyCode::Backslash), ("K_SEMICOLON", KeyCode::Semicolon),
            ("K_QUOTE", KeyCode::Quote), ("K_BACKQUOTE", KeyCode::Backquote),
            ("K_COMMA", KeyCode::Comma), ("K_PERIOD", KeyCode::Period),
            ("K_SLASH", KeyCode::Slash),
            // 小键盘
            ("K_KP0", KeyCode::Numpad0), ("K_KP1", KeyCode::Numpad1),
            ("K_KP2", KeyCode::Numpad2), ("K_KP3", KeyCode::Numpad3),
            ("K_KP4", KeyCode::Numpad4), ("K_KP5", KeyCode::Numpad5),
            ("K_KP6", KeyCode::Numpad6), ("K_KP7", KeyCode::Numpad7),
            ("K_KP8", KeyCode::Numpad8), ("K_KP9", KeyCode::Numpad9),
            ("K_KP_PLUS", KeyCode::NumpadAdd), ("K_KP_MINUS", KeyCode::NumpadSubtract),
            ("K_KP_MULTIPLY", KeyCode::NumpadMultiply), ("K_KP_DIVIDE", KeyCode::NumpadDivide),
            ("K_KP_ENTER", KeyCode::NumpadEnter), ("K_KP_PERIOD", KeyCode::NumpadDecimal),
            // 其他
            ("K_PRINT", KeyCode::PrintScreen), ("K_SCROLLLOCK", KeyCode::ScrollLock),
            ("K_PAUSE", KeyCode::Pause), ("K_NUMLOCK", KeyCode::NumLock),
        ];
        ///先补挂子模块（自动版被用户 exec 顶替），再注册
        /// K_* typed 常量到 pygame 顶层。Result
        pub fn module_exec(vm: &VirtualMachine, module: &Py<PyModule>) -> PyResult<()> {
            __init_attributes(vm, module);
            for (name, kc) in KEY_TABLE {
                module.set_attr(*name, PyKey(*kc).into_pyobject(vm), vm)?;
            }
            Ok(())
        }

        // ── 修饰键位常量（位值自定，承诺名字与语义）──
        #[pyattr]
        pub(crate) const KMOD_SHIFT: u32 = starfish::pygame::key::KMOD_SHIFT;
        #[pyattr]
        pub(crate) const KMOD_CTRL: u32 = starfish::pygame::key::KMOD_CTRL;
        #[pyattr]
        pub(crate) const KMOD_ALT: u32 = starfish::pygame::key::KMOD_ALT;
        #[pyattr]
        pub(crate) const KMOD_META: u32 = starfish::pygame::key::KMOD_META;


        #[derive(Debug, PyPayload)]
        #[pyclass(no_attr, module = false, name = "Pressed", unhashable = true)]
        pub(crate) struct PyPressed;

        #[pyclass]
        impl PyPressed {
            #[pymethod]
            fn get(&self, key: PyRef<PyKey>) -> bool {
                starfish::pygame::key::get_pressed().get(key.0)
            }
        }


        /// pygame.key.get_pressed() → Pressed 视图（.get(K_x) 查按下态）。
        #[pyfunction]
        fn get_pressed(vm: &VirtualMachine) -> PyResult<PyRef<PyPressed>> {
            Ok(PyPressed.into_ref(&vm.ctx))
        }

        /// 修饰键位集（KMOD_* 位；KeyDown/KeyUp 事件喂）。
        #[pyfunction]
        fn get_mods(_vm: &VirtualMachine) -> u32 {
            starfish::pygame::key::get_mods()
        }

        /// 键名（W3C KeyboardEvent.code 命名——name(K_x) = "KeyX" 等）。
        #[pyfunction]
        fn name(key: PyRef<PyKey>, _vm: &VirtualMachine) -> String {
            starfish::pygame::key::name(key.0)
        }

        /// 窗口键盘聚焦态。
        #[pyfunction]
        fn get_focused(_vm: &VirtualMachine) -> bool {
            starfish::pygame::key::get_focused()
        }

        /// 键重复软件合成（垫底：自动重复由 OS repeat 标记承载，收下不生效）。
        #[pyfunction]
        fn set_repeat(_delay: u32, _interval: u32) {}

        /// 键重复配置查询（垫底：(0, 0) = 未启用软件重复）。
        #[pyfunction]
        fn get_repeat(_vm: &VirtualMachine) -> (u32, u32) {
            (0, 0)
        }


    }

    // ── mouse：位置 / 按键查询（状态由 starfish 事件泵喂）────────────
    #[pymodule]
    pub(crate) mod mouse {
        use crate::rpy::args::SfPoint;
        use rustpython_vm::VirtualMachine;

        /// 鼠标位置（客户区物理像素，y 向下；事件泵前为 (0, 0)）。
        #[pyfunction]
        fn get_pos(_vm: &VirtualMachine) -> (i32, i32) {
            starfish::pygame::mouse::get_pos()
        }

        /// 左/中/右三键按下态（pygame 惯例）。
        #[pyfunction]
        fn get_pressed(_vm: &VirtualMachine) -> (bool, bool, bool) {
            starfish::pygame::mouse::get_pressed()
        }

        /// 相对位移（自上次 get_rel 调用起；连续调用逐段累加）。
        #[pyfunction]
        fn get_rel(_vm: &VirtualMachine) -> (i32, i32) {
            starfish::pygame::mouse::get_rel()
        }

        /// 光标可见态（窗口层 set_cursor_visible；移动端无光标语义）。
        #[pyfunction]
        fn set_visible(visible: bool, _vm: &VirtualMachine) {
            starfish::pygame::mouse::set_visible(visible)
        }

        /// 光标可见态查询。
        #[pyfunction]
        fn get_visible(_vm: &VirtualMachine) -> bool {
            starfish::pygame::mouse::get_visible()
        }

        /// 窗口鼠标聚焦态（简化：与键盘聚焦同源）。
        #[pyfunction]
        fn get_focused(_vm: &VirtualMachine) -> bool {
            starfish::pygame::mouse::get_focused()
        }

        /// warp 光标（垫底：仅更新状态表，不扭曲真实光标、无合成事件
        /// ——starfish-window v1 无光标扭曲能力）。
        #[pyfunction]
        fn set_pos(pos: SfPoint, _vm: &VirtualMachine) {
            starfish::pygame::mouse::set_pos(pos.0, pos.1)
        }
    }

    // ── gamepad：手柄查询（base gamepad 状态表，宿主泵每帧排水）──────
    // 命名定约（架构文档 §二）：替代 joystick+controller 两名，按钮语义
    // 对齐 SDL/标准布局。常量 = 整数索引（pygame.controller 同惯例；
    // Web Gamepad API 同序，映射表 = starfish BUTTON_ORDER/AXIS_ORDER，
    // from_index 出界 ValueError）。Android/iOS 后端恒空表（平台矩阵）。
    #[pymodule]
    pub(crate) mod gamepad {
        use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine};

        use starfish::base::gamepad::{Axis, Button};

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
            starfish::pygame::gamepad::get_count()
        }

        /// 已连接手柄编号列表（连接顺序；平台内自增）。
        #[pyfunction]
        fn connected(vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let ids: Vec<PyObjectRef> = starfish::pygame::gamepad::connected()
                .into_iter()
                .map(|id| vm.ctx.new_int(id as i64).into())
                .collect();
            Ok(vm.ctx.new_list(ids).into())
        }

        /// 第一个连接的手柄编号（单手柄便捷入口；无 = None）。
        #[pyfunction]
        fn primary(_vm: &VirtualMachine) -> Option<usize> {
            starfish::pygame::gamepad::primary()
        }

        /// 指定手柄是否已连接。
        #[pyfunction]
        fn is_connected(id: usize, _vm: &VirtualMachine) -> bool {
            starfish::pygame::gamepad::is_connected(id)
        }

        /// 按键按下态（未连接 = False）。
        #[pyfunction]
        fn get_pressed(id: usize, btn: u8, vm: &VirtualMachine) -> PyResult<bool> {
            Ok(starfish::pygame::gamepad::is_pressed(id, map_btn(btn, vm)?))
        }

        /// 本帧新按下（上帧未按下；未连接 = False）。
        #[pyfunction]
        fn just_pressed(id: usize, btn: u8, vm: &VirtualMachine) -> PyResult<bool> {
            Ok(starfish::pygame::gamepad::just_pressed(id, map_btn(btn, vm)?))
        }

        /// 轴值（摇杆 -1.0..=1.0；扳机 0..=1.0；未连接 = 0.0）。
        #[pyfunction]
        fn get_axis(id: usize, axis: u8, vm: &VirtualMachine) -> PyResult<f32> {
            Ok(starfish::pygame::gamepad::axis(id, map_axis(axis, vm)?))
        }
    }

    // ── touch：多点触控手指查询（批次十三手指状态表的 Python 面）──────
    // 定约差异：pygame 官方按"设备→手指"两级编号（SDL 触屏设备概念），
    // 本层按**手指 id** 单层组织（现代移动惯例，多点语义直接可用——
    // starfish touch.rs 既定决策）。桌面端恒空表（仅移动端产生 Touch
    // 事件；主手指已合成鼠标事件，交互逻辑可复用 mouse）。
    #[pymodule]
    pub(crate) mod touch {
        use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine};

        /// 当前按住的手指数（0 = 无触摸；桌面恒 0）。
        #[pyfunction]
        fn get_count(_vm: &VirtualMachine) -> usize {
            starfish::pygame::touch::get_count()
        }

        /// 活动手指快照：[{"id", "x", "y", "phase"}] 按 id 升序。
        /// phase 仅 "started"/"moved"——抬起的手指即出表（Ended/
        /// Cancelled 不驻留）。
        #[pyfunction]
        fn get_fingers(vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let mut items: Vec<PyObjectRef> = Vec::new();
            for f in starfish::pygame::touch::fingers() {
                let d = vm.ctx.new_dict();
                d.set_item("id", vm.ctx.new_int(f.id as i64).into(), vm)?;
                d.set_item("x", vm.ctx.new_int(f.x as i64).into(), vm)?;
                d.set_item("y", vm.ctx.new_int(f.y as i64).into(), vm)?;
                let phase = match f.phase {
                    starfish::base::window::TouchPhase::Started => "started",
                    starfish::base::window::TouchPhase::Moved => "moved",
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
            starfish::pygame::touch::get_pos(finger_id)
        }
    }

    // ── io：文件读写（批次 M；原生同步直读，含基准目录语义——
    // Android = 引擎 run 自动注入的应用私有目录，绝对路径原样）──
    // Web v1 不提供同步面（fetch 真异步，Python 同步面无法承载——
    // 用宿主预加载/资源句柄模式，同 Font("cjk") 先例），调用即报错。
    #[pymodule]
    pub(crate) mod io {
        use rustpython_vm::convert::TryFromObject;
        use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine, builtins::PyStrRef};

        fn path_str<'a>(p: &'a PyStrRef, vm: &VirtualMachine) -> PyResult<&'a str> {
            p.to_str().ok_or_else(|| vm.new_value_error("路径含非 UTF-8 字符"))
        }

        /// read(path) -> bytes
        #[pyfunction]
        fn read(path: PyStrRef, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            #[cfg(target_arch = "wasm32")]
            return Err(vm.new_value_error(
                "pygame.io: Web 端 v1 不提供同步 io（宿主预加载/异步资源装载承载）",
            ));
            #[cfg(not(target_arch = "wasm32"))]
            {
                let data = pollster::block_on(starfish::base::io::read(path_str(&path, vm)?))
                    .map_err(|e| vm.new_value_error(format!("io 读取失败: {e}")))?;
                Ok(vm.ctx.new_bytes(data).into())
            }
        }

        /// read_text(path) -> str（UTF-8，非法序列替换）
        #[pyfunction]
        fn read_text(path: PyStrRef, vm: &VirtualMachine) -> PyResult<String> {
            #[cfg(target_arch = "wasm32")]
            return Err(vm.new_value_error(
                "pygame.io: Web 端 v1 不提供同步 io（宿主预加载/异步资源装载承载）",
            ));
            #[cfg(not(target_arch = "wasm32"))]
            {
                let data = pollster::block_on(starfish::base::io::read(path_str(&path, vm)?))
                    .map_err(|e| vm.new_value_error(format!("io 读取失败: {e}")))?;
                Ok(String::from_utf8_lossy(&data).into_owned())
            }
        }

        /// write(path, data)：覆盖写（bytes 或 str——str 按 UTF-8 编码）
        #[pyfunction]
        fn write(path: PyStrRef, data: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
            #[cfg(target_arch = "wasm32")]
            return Err(vm.new_value_error(
                "pygame.io: Web 端 v1 不提供同步 io（宿主预加载/异步资源装载承载）",
            ));
            #[cfg(not(target_arch = "wasm32"))]
            {
                let bytes: Vec<u8> = if let Some(s) = data.downcast_ref::<rustpython_vm::builtins::PyStr>()
                {
                    s.to_str().ok_or_else(|| vm.new_value_error("内容含非 UTF-8 字符"))?.as_bytes().to_vec()
                } else {
                    data.try_bytes_like(vm, |b| b.to_vec())?
                };
                pollster::block_on(starfish::base::io::write(path_str(&path, vm)?, bytes))
                    .map_err(|e| vm.new_value_error(format!("io 写入失败: {e}")))
            }
        }

        /// exists(path) -> bool
        #[pyfunction]
        fn exists(path: PyStrRef, vm: &VirtualMachine) -> PyResult<bool> {
            #[cfg(target_arch = "wasm32")]
            return Err(vm.new_value_error(
                "pygame.io: Web 端 v1 不提供同步 io（宿主预加载/异步资源装载承载）",
            ));
            #[cfg(not(target_arch = "wasm32"))]
            pollster::block_on(starfish::base::io::exists(path_str(&path, vm)?))
                .map_err(|e| vm.new_value_error(format!("io exists 失败: {e}")))
        }

        /// write_text(path, text)：覆盖写文本（UTF-8）
        #[pyfunction]
        fn write_text(path: PyStrRef, text: PyStrRef, vm: &VirtualMachine) -> PyResult<()> {
            #[cfg(target_arch = "wasm32")]
            return Err(vm.new_value_error(
                "pygame.io: Web 端 v1 不提供同步 io（宿主预加载/异步资源装载承载）",
            ));
            #[cfg(not(target_arch = "wasm32"))]
            {
                let t = text
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("内容含非 UTF-8 字符"))?;
                pollster::block_on(starfish::base::io::write_text(path_str(&path, vm)?, t))
                    .map_err(|e| vm.new_value_error(format!("io 写入失败: {e}")))
            }
        }

        /// set_base_dir(dir)：相对路径基准目录（覆盖写；Android 由引擎自动
        /// 注入应用私有目录，一般无需手动调）
        #[pyfunction]
        fn set_base_dir(dir: PyStrRef, vm: &VirtualMachine) -> PyResult<()> {
            #[cfg(target_arch = "wasm32")]
            return Err(vm.new_value_error("pygame.io: Web 端基准 = URL 前缀，无同步设置面"));
            #[cfg(not(target_arch = "wasm32"))]
            {
                starfish::base::io::set_base_dir(path_str(&dir, vm)?);
                Ok(())
            }
        }
    }

    // ── net：网络（批次 M；轮询式语义——帧内 send/recv，主线程永不阻塞）──
    // TCP = 消息分帧连接（4 字节大端长度前缀，单帧上限 16 MB；原生后台
    // 线程 + std 阻塞流，Web 自动 WebSocket——三平台同一轮询面）。
    // UDP = 原生专属（Web 显式不支持）。
    #[pymodule]
    pub(crate) mod net {
        use rustpython_derive::pyclass;
        use rustpython_vm::convert::TryFromObject;
        use rustpython_vm::types::Constructor;
        use rustpython_vm::{PyObjectRef, Py, PyPayload, PyResult, VirtualMachine, builtins::{PyStrRef, PyType}};
        use std::cell::RefCell;

        use starfish::base::net::{ConnState, TcpConn, UdpSock};

        fn data_bytes(data: PyObjectRef, vm: &VirtualMachine) -> PyResult<Vec<u8>> {
            if let Some(s) = data.downcast_ref::<rustpython_vm::builtins::PyStr>() {
                return Ok(s
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("内容含非 UTF-8 字符"))?
                    .as_bytes()
                    .to_vec());
            }
            data.try_bytes_like(vm, |b| b.to_vec())
        }

        /// TCP(addr)：消息分帧连接（addr = "host:port"；Web = "ws://host:port"）。
        /// connect 立即返回（后台握手），Connecting 期间 send 入队缓冲。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.net", name = "TCP", unhashable = true)]
        pub(crate) struct PyTcp(RefCell<TcpConn>);

        impl std::fmt::Debug for PyTcp {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("TCP")
            }
        }

        #[pyclass(with(Constructor))]
        impl PyTcp {
            #[pymethod]
            fn send(&self, data: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
                self.0
                    .borrow_mut()
                    .send(&data_bytes(data, vm)?)
                    .map_err(|e| vm.new_value_error(format!("net 发送失败: {e}")))
            }

            /// recv() -> bytes | None：收一帧（非阻塞；无帧 = None，下帧再查）
            #[pymethod]
            fn recv(&self, vm: &VirtualMachine) -> PyResult<Option<PyObjectRef>> {
                Ok(self
                    .0
                    .borrow_mut()
                    .try_recv()
                    .map(|b| vm.ctx.new_bytes(b).into()))
            }

            /// state() -> "connecting" | "connected" | "closed"
            #[pymethod]
            fn state(&self) -> &'static str {
                match self.0.borrow().state() {
                    ConnState::Connecting => "connecting",
                    ConnState::Connected => "connected",
                    ConnState::Closed => "closed",
                }
            }

            #[pymethod]
            fn close(&self) {
                self.0.borrow_mut().close();
            }
        }

        impl Constructor for PyTcp {
            type Args = (String,);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                vm: &VirtualMachine,
            ) -> PyResult<Self> {
                let conn = TcpConn::connect(&args.0)
                    .map_err(|e| vm.new_value_error(format!("net 连接失败: {e}")))?;
                Ok(Self(RefCell::new(conn)))
            }
        }

        /// UDP(local_addr)：非阻塞数据报套接字（原生平台；Web 调用即报错）。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.net", name = "UDP", unhashable = true)]
        pub(crate) struct PyUdp(RefCell<UdpSock>);

        impl std::fmt::Debug for PyUdp {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("UDP")
            }
        }

        #[pyclass(with(Constructor))]
        impl PyUdp {
            /// send_to(data, peer)：对端 "host:port"。
            #[pymethod]
            fn send_to(
                &self,
                data: PyObjectRef,
                peer: PyStrRef,
                vm: &VirtualMachine,
            ) -> PyResult<()> {
                #[cfg(target_arch = "wasm32")]
                return Err(vm.new_value_error("pygame.net: Web 端不支持 UDP"));
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let peer = peer
                        .to_str()
                        .ok_or_else(|| vm.new_value_error("对端地址含非 UTF-8 字符"))?;
                    self.0
                        .borrow_mut()
                        .send_to(&data_bytes(data, vm)?, peer)
                        .map_err(|e| vm.new_value_error(format!("net 发送失败: {e}")))
                }
            }

            /// recv_from() -> (bytes, addr) | None：收一报（非阻塞）。
            #[pymethod]
            fn recv_from(&self, vm: &VirtualMachine) -> PyResult<Option<PyObjectRef>> {
                #[cfg(target_arch = "wasm32")]
                return Err(vm.new_value_error("pygame.net: Web 端不支持 UDP"));
                #[cfg(not(target_arch = "wasm32"))]
                {
                    Ok(self.0.borrow_mut().try_recv_from().map(|(data, addr)| {
                        let bytes: PyObjectRef = vm.ctx.new_bytes(data).into();
                        let peer: PyObjectRef = vm.ctx.new_str(addr.to_string()).into();
                        vm.new_tuple((bytes, peer)).into()
                    }))
                }
            }

            /// set_broadcast(on)：广播开关。
            #[pymethod]
            fn set_broadcast(&self, on: bool, vm: &VirtualMachine) -> PyResult<()> {
                #[cfg(target_arch = "wasm32")]
                return Err(vm.new_value_error("pygame.net: Web 端不支持 UDP"));
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = vm;
                    self.0
                        .borrow_mut()
                        .set_broadcast(on)
                        .map_err(|e| vm.new_value_error(format!("net set_broadcast 失败: {e}")))
                }
            }
        }

        impl Constructor for PyUdp {
            type Args = (String,);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                vm: &VirtualMachine,
            ) -> PyResult<Self> {
                #[cfg(target_arch = "wasm32")]
                return Err(vm.new_value_error("pygame.net: Web 端不支持 UDP"));
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let sock = UdpSock::bind(&args.0)
                        .map_err(|e| vm.new_value_error(format!("net 绑定失败: {e}")))?;
                    Ok(Self(RefCell::new(sock)))
                }
            }
        }
    }

    // ── dialog：文件打开 / 保存（批次 M；桌面模态——rfd 内置模态消息
    // 循环，阻塞当前帧但窗口保持响应）。Web（input+下载）与 Android
    //（SAF 轮询式 job）v1 不提供同步面，调用即报错——轮询式 job API
    // 为后续批次）──
    #[pymodule]
    pub(crate) mod dialog {
        use rustpython_derive::pyclass;
        use rustpython_vm::convert::TryFromObject;
        use rustpython_vm::{PyObjectRef, PyPayload, PyResult, VirtualMachine, builtins::PyStrRef};

        use starfish::base::dialog::PickedFile;

        /// PickedFile：用户选取的文件（桌面持路径惰性读盘）。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.dialog", name = "PickedFile", unhashable = true)]
        pub(crate) struct PyPickedFile(PickedFile);

        impl std::fmt::Debug for PyPickedFile {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("PickedFile")
            }
        }

        #[pyclass]
        impl PyPickedFile {
            /// 文件名（含扩展名）。
            #[pymethod]
            fn name(&self) -> String {
                self.0.name().to_owned()
            }

            /// read() -> bytes：读取全部字节。
            #[pymethod]
            fn read(&self, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
                let data = self
                    .0
                    .read()
                    .map_err(|e| vm.new_value_error(format!("dialog 读取失败: {e}")))?;
                Ok(vm.ctx.new_bytes(data).into())
            }
        }

        /// 过滤项 [(展示名, [扩展名, ...]), ...] → dialog Filter 标记
        /// 生命周期（&'static）——一次性用户动作，Box::leak 有意泄漏
        ///（次数有界，进程级回收）。
        fn leak_filters(
            filters: &[(String, Vec<String>)],
        ) -> Vec<(&'static str, &'static [&'static str])> {
            filters
                .iter()
                .map(|(name, exts)| {
                    let name: &'static str = Box::leak(name.clone().into_boxed_str());
                    let exts: Vec<&'static str> = exts
                        .iter()
                        .map(|e| Box::leak(e.clone().into_boxed_str()) as &'static str)
                        .collect();
                    (name, Box::leak(exts.into_boxed_slice()) as &'static [&'static str])
                })
                .collect()
        }

        /// open_file(title=None, filters=None) -> PickedFile | None
        ///（None = 用户取消。filters = [("文本文件", ["txt", "md"]), ...]）
        #[pyfunction]
        fn open_file(args: OpenFileArgs, vm: &VirtualMachine) -> PyResult<Option<PyPickedFile>> {
            #[cfg(not(any(
                target_os = "windows",
                target_os = "linux",
                target_os = "macos"
            )))]
            return Err(vm.new_value_error(
                "pygame.dialog: v1 同步对话框为桌面专属（Web/Android 走异步 job API，后续批次）",
            ));
            #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
            {
                // 过滤项手工解包（FromArgs 通道对嵌套 Vec<tuple> 不通，
                // RP 亦无 Vec/tuple 的 TryFromObject——downcast 元组后
                // as_slice 逐元素提取，extract_elements_with 同 sndarray）
                let filters: Vec<(String, Vec<String>)> = match &args.filters {
                    Some(o) => vm.extract_elements_with(o, |el| {
                        let t = el.downcast::<rustpython_vm::builtins::PyTuple>()
                            .map_err(|_| vm.new_type_error("过滤项应为 (展示名, [扩展名...]) 二元组"))?;
                        let items = t.as_slice();
                        if items.len() != 2 {
                            return Err(vm.new_type_error("过滤项应为二元组 (展示名, [扩展名...])"));
                        }
                        let name = items[0]
                            .downcast_ref::<rustpython_vm::builtins::PyStr>()
                            .ok_or_else(|| vm.new_type_error("过滤项[0] 应为展示名 str"))?
                            .to_str()
                            .ok_or_else(|| vm.new_value_error("展示名含非 UTF-8 字符"))?
                            .to_owned();
                        let exts: Vec<String> = vm.extract_elements_with(&items[1], |e| {
                            e.downcast_ref::<rustpython_vm::builtins::PyStr>()
                                .ok_or_else(|| vm.new_type_error("扩展名应为 str"))?
                                .to_str()
                                .ok_or_else(|| vm.new_value_error("扩展名含非 UTF-8 字符"))
                                .map(String::from)
                        })?;
                        Ok((name, exts))
                    })?,
                    None => Vec::new(),
                };
                let filters = leak_filters(&filters);
                let picked = pollster::block_on(starfish::base::dialog::pick_file(
                    args.title.as_deref(),
                    &filters,
                ))
                .map_err(|e| vm.new_value_error(format!("dialog: {e}")))?;
                Ok(picked.map(PyPickedFile))
            }
        }

        #[derive(rustpython_derive::FromArgs)]
        pub(crate) struct OpenFileArgs {
            /// 对话框标题（None = 系统默认）
            #[pyarg(any, optional)]
            pub(crate) title: Option<String>,
            /// 过滤项 [(展示名, [扩展名...]), ...]（PyObjectRef 手工解包）
            #[pyarg(any, optional)]
            pub(crate) filters: Option<PyObjectRef>,
        }

        /// save_bytes(file_name, data) -> bool：保存到用户指定位置
        ///（True = 已保存 / False = 用户取消。桌面 = 另存为对话框；
        /// Web 语义 = 触发下载——v1 桌面专属）。
        #[pyfunction]
        fn save_bytes(
            file_name: PyStrRef,
            data: PyObjectRef,
            vm: &VirtualMachine,
        ) -> PyResult<bool> {
            #[cfg(not(any(
                target_os = "windows",
                target_os = "linux",
                target_os = "macos"
            )))]
            return Err(vm.new_value_error(
                "pygame.dialog: v1 同步对话框为桌面专属（Web/Android 走异步 job API，后续批次）",
            ));
            #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
            {
                let name = file_name
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("文件名含非 UTF-8 字符"))?;
                let bytes = data_bytes(data, vm)?;
                let saved = pollster::block_on(starfish::base::dialog::save_bytes(name, bytes))
                    .map_err(|e| vm.new_value_error(format!("dialog: {e}")))?;
                Ok(saved.is_some())
            }
        }

        fn data_bytes(data: PyObjectRef, vm: &VirtualMachine) -> PyResult<Vec<u8>> {
            if let Some(s) = data.downcast_ref::<rustpython_vm::builtins::PyStr>() {
                return Ok(s
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("内容含非 UTF-8 字符"))?
                    .as_bytes()
                    .to_vec());
            }
            data.try_bytes_like(vm, |b| b.to_vec())
        }
    }

    // ── gfxdraw：SDL_gfx 风格兼容签名面（契约原则 3——一个实现多个
    // 兼容名；gfxdraw 独立栅格与 AA 语义已并入 draw/wgpu 采样，本模块
    // 只是签名适配：目标会话在前、坐标分离、颜色在后；aa* = 兼容别名，
    // AA 由 wgpu 线性采样承载，与 f.circle(...) 同一 batch 零额外成本）──
    #[pymodule]
    pub(crate) mod gfxdraw {
        use super::{PyScreenSession, PySurfaceSession};
        use crate::rpy::args::{SfColor, SfPoints, SfRect};
        use crate::rpy::bridge;
        use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine};

        /// gfxdraw 目标：SurfaceSession 直绘（自有 batch）；Screen 会话
        /// （PyScreenSession）= 当前活跃会话（with_session 路由，须在 with 内）。
        enum Target<'a> {
            Surf(&'a PySurfaceSession),
            Active,
        }

        fn route<'a>(f: &'a PyObjectRef, vm: &VirtualMachine) -> PyResult<Target<'a>> {
            if let Some(ss) = f.downcast_ref::<PySurfaceSession>() {
                return Ok(Target::Surf(ss));
            }
            if f.downcast_ref::<PyScreenSession>().is_some() {
                return Ok(Target::Active);
            }
            Err(vm.new_type_error(
                "gfxdraw 目标应为渲染会话（with ...render() as f 的 f）",
            ))
        }

        /// pixel(f, x, y, color)
        #[pyfunction]
        fn pixel(f: PyObjectRef, x: i32, y: i32, color: SfColor, vm: &VirtualMachine) -> PyResult<()> {
            let c = bridge::color_of(color);
            match route(&f, vm)? {
                Target::Surf(s) => {
                    starfish::pygame::draw::rect(&*s.surface, c,
                        starfish::pygame::Rect::new(x, y, 1, 1), 0)
                }
                Target::Active => bridge::with_session(vm, |t| {
                    starfish::pygame::draw::rect(t, c,
                        starfish::pygame::Rect::new(x, y, 1, 1), 0)
                })?,
            };
            Ok(())
        }

        /// hline(f, x1, x2, y, color)
        #[pyfunction]
        fn hline(
            f: PyObjectRef,
            x1: i32,
            x2: i32,
            y: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            line(f, x1, y, x2, y, color, vm)
        }

        /// vline(f, x, y1, y2, color)
        #[pyfunction]
        fn vline(
            f: PyObjectRef,
            x: i32,
            y1: i32,
            y2: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            line(f, x, y1, x, y2, color, vm)
        }

        /// line(f, x1, y1, x2, y2, color)
        #[pyfunction]
        fn line(
            f: PyObjectRef,
            x1: i32,
            y1: i32,
            x2: i32,
            y2: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            let c = bridge::color_of(color);
            match route(&f, vm)? {
                Target::Surf(s) => starfish::pygame::draw::line(&*s.surface, c, (x1, y1), (x2, y2), 1),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish::pygame::draw::line(t, c, (x1, y1), (x2, y2), 1)
                })?,
            };
            Ok(())
        }

        /// aaline(f, x1, y1, x2, y2, color)：抗锯齿线 = 兼容别名
        ///（AA 由 wgpu 线性采样承载，同实现）。
        #[pyfunction]
        fn aaline(
            f: PyObjectRef,
            x1: i32,
            y1: i32,
            x2: i32,
            y2: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            line(f, x1, y1, x2, y2, color, vm)
        }

        /// circle(f, x, y, radius, color)：1px 描边圆。
        #[pyfunction]
        fn circle(
            f: PyObjectRef,
            x: i32,
            y: i32,
            radius: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            let c = bridge::color_of(color);
            let r = radius.max(0) as u32;
            match route(&f, vm)? {
                Target::Surf(s) => starfish::pygame::draw::circle(&*s.surface, c, (x, y), r, 1),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish::pygame::draw::circle(t, c, (x, y), r, 1)
                })?,
            };
            Ok(())
        }

        /// aacircle(f, x, y, radius, color)：抗锯齿圆 = 兼容别名。
        #[pyfunction]
        fn aacircle(
            f: PyObjectRef,
            x: i32,
            y: i32,
            radius: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            circle(f, x, y, radius, color, vm)
        }

        /// filled_circle(f, x, y, radius, color)：填充圆。
        #[pyfunction]
        fn filled_circle(
            f: PyObjectRef,
            x: i32,
            y: i32,
            radius: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            let c = bridge::color_of(color);
            let r = radius.max(0) as u32;
            match route(&f, vm)? {
                Target::Surf(s) => starfish::pygame::draw::circle(&*s.surface, c, (x, y), r, 0),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish::pygame::draw::circle(t, c, (x, y), r, 0)
                })?,
            };
            Ok(())
        }

        /// ellipse(f, x, y, rx, ry, color)：1px 描边椭圆（中心 + 半径轴）。
        #[pyfunction]
        fn ellipse(
            f: PyObjectRef,
            x: i32,
            y: i32,
            rx: i32,
            ry: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            let c = bridge::color_of(color);
            let rect = starfish::pygame::Rect::new(x - rx, y - ry, rx.max(0) * 2, ry.max(0) * 2);
            match route(&f, vm)? {
                Target::Surf(s) => starfish::pygame::draw::ellipse(&*s.surface, c, rect, 1),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish::pygame::draw::ellipse(t, c, rect, 1)
                })?,
            };
            Ok(())
        }

        /// aaellipse(f, x, y, rx, ry, color)：抗锯齿椭圆 = 兼容别名。
        #[pyfunction]
        fn aaellipse(
            f: PyObjectRef,
            x: i32,
            y: i32,
            rx: i32,
            ry: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            ellipse(f, x, y, rx, ry, color, vm)
        }

        /// filled_ellipse(f, x, y, rx, ry, color)：填充椭圆。
        #[pyfunction]
        fn filled_ellipse(
            f: PyObjectRef,
            x: i32,
            y: i32,
            rx: i32,
            ry: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            let c = bridge::color_of(color);
            let rect = starfish::pygame::Rect::new(x - rx, y - ry, rx.max(0) * 2, ry.max(0) * 2);
            match route(&f, vm)? {
                Target::Surf(s) => starfish::pygame::draw::ellipse(&*s.surface, c, rect, 0),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish::pygame::draw::ellipse(t, c, rect, 0)
                })?,
            };
            Ok(())
        }

        /// arc(f, x, y, radius, start_angle, stop_angle, color)：椭圆弧
        ///（**度数制**——SDL_gfx 惯例；1px 描边，v1 简化）。
        #[pyfunction]
        fn arc(
            f: PyObjectRef,
            x: i32,
            y: i32,
            radius: i32,
            start_angle: i32,
            stop_angle: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            let c = bridge::color_of(color);
            let rect = starfish::pygame::Rect::new(x - radius, y - radius, radius.max(0) * 2, radius.max(0) * 2);
            let (s, e) = (
                (start_angle as f32).to_radians(),
                (stop_angle as f32).to_radians(),
            );
            match route(&f, vm)? {
                Target::Surf(sfc) => starfish::pygame::draw::arc(&*sfc.surface, c, rect, s, e, 1),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish::pygame::draw::arc(t, c, rect, s, e, 1)
                })?,
            };
            Ok(())
        }

        /// trigon 坐标组（9 参超 RP 原生函数实现上限——FromArgs 结构体
        /// 按声明序扁平展开，Python 签名不变）
        #[derive(rustpython_derive::FromArgs)]
        pub(crate) struct TrigonArgs {
            #[pyarg(any)]
            pub(crate) x1: i32,
            #[pyarg(any)]
            pub(crate) y1: i32,
            #[pyarg(any)]
            pub(crate) x2: i32,
            #[pyarg(any)]
            pub(crate) y2: i32,
            #[pyarg(any)]
            pub(crate) x3: i32,
            #[pyarg(any)]
            pub(crate) y3: i32,
        }

        /// trigon(f, x1, y1, x2, y2, x3, y3, color)：1px 描边三角。
        #[pyfunction]
        fn trigon(f: PyObjectRef, args: TrigonArgs, color: SfColor, vm: &VirtualMachine) -> PyResult<()> {
            filled_polygon3(&f, args.x1, args.y1, args.x2, args.y2, args.x3, args.y3, color, vm)?;
            // 描边三角 = 填充 + 1px 轮廓（gfxdraw trigon 语义）
            polygon3(&f, args.x1, args.y1, args.x2, args.y2, args.x3, args.y3, color, vm)
        }

        /// aatrigon(...)：抗锯齿三角 = 兼容别名（同 trigon 组合）。
        #[pyfunction]
        fn aatrigon(f: PyObjectRef, args: TrigonArgs, color: SfColor, vm: &VirtualMachine) -> PyResult<()> {
            trigon(f, args, color, vm)
        }

        /// filled_trigon(f, x1, y1, x2, y2, x3, y3, color)：填充三角。
        #[pyfunction]
        fn filled_trigon(f: PyObjectRef, args: TrigonArgs, color: SfColor, vm: &VirtualMachine) -> PyResult<()> {
            filled_polygon3(&f, args.x1, args.y1, args.x2, args.y2, args.x3, args.y3, color, vm)
        }

        #[allow(clippy::too_many_arguments)]
        fn filled_polygon3(
            f: &PyObjectRef,
            x1: i32,
            y1: i32,
            x2: i32,
            y2: i32,
            x3: i32,
            y3: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            let c = bridge::color_of(color);
            let pts = SfPoints(vec![(x1, y1), (x2, y2), (x3, y3)]);
            match route(f, vm)? {
                Target::Surf(s) => starfish::pygame::draw::polygon(&*s.surface, c, &pts.0, 0),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish::pygame::draw::polygon(t, c, &pts.0, 0)
                })?,
            };
            Ok(())
        }

        #[allow(clippy::too_many_arguments)]
        fn polygon3(
            f: &PyObjectRef,
            x1: i32,
            y1: i32,
            x2: i32,
            y2: i32,
            x3: i32,
            y3: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            let c = bridge::color_of(color);
            let pts = SfPoints(vec![(x1, y1), (x2, y2), (x3, y3)]);
            match route(f, vm)? {
                Target::Surf(s) => starfish::pygame::draw::polygon(&*s.surface, c, &pts.0, 1),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish::pygame::draw::polygon(t, c, &pts.0, 1)
                })?,
            };
            Ok(())
        }

        /// polygon(f, points, color)：1px 描边多边形（points = [(x, y), ...]）。
        #[pyfunction]
        fn polygon(f: PyObjectRef, points: SfPoints, color: SfColor, vm: &VirtualMachine) -> PyResult<()> {
            let c = bridge::color_of(color);
            match route(&f, vm)? {
                Target::Surf(s) => starfish::pygame::draw::polygon(&*s.surface, c, &points.0, 1),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish::pygame::draw::polygon(t, c, &points.0, 1)
                })?,
            };
            Ok(())
        }

        /// aapolygon(f, points, color)：抗锯齿多边形 = 兼容别名。
        #[pyfunction]
        fn aapolygon(f: PyObjectRef, points: SfPoints, color: SfColor, vm: &VirtualMachine) -> PyResult<()> {
            polygon(f, points, color, vm)
        }

        /// filled_polygon(f, points, color)：填充多边形。
        #[pyfunction]
        fn filled_polygon(
            f: PyObjectRef,
            points: SfPoints,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            let c = bridge::color_of(color);
            match route(&f, vm)? {
                Target::Surf(s) => starfish::pygame::draw::polygon(&*s.surface, c, &points.0, 0),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish::pygame::draw::polygon(t, c, &points.0, 0)
                })?,
            };
            Ok(())
        }

        /// rectangle(f, rect, color)：1px 描边矩形（rect = (left, top, w, h)）。
        #[pyfunction]
        fn rectangle(f: PyObjectRef, rect: SfRect, color: SfColor, vm: &VirtualMachine) -> PyResult<()> {
            let c = bridge::color_of(color);
            let r = starfish::pygame::Rect::new(rect.x, rect.y, rect.w, rect.h);
            match route(&f, vm)? {
                Target::Surf(s) => starfish::pygame::draw::rect(&*s.surface, c, r, 1),
                Target::Active => bridge::with_session(vm, |t| starfish::pygame::draw::rect(t, c, r, 1))?,
            };
            Ok(())
        }

        /// box(f, rect, color)：填充矩形。
        #[pyfunction(name = "box")]
        fn box_(f: PyObjectRef, rect: SfRect, color: SfColor, vm: &VirtualMachine) -> PyResult<()> {
            let c = bridge::color_of(color);
            let r = starfish::pygame::Rect::new(rect.x, rect.y, rect.w, rect.h);
            match route(&f, vm)? {
                Target::Surf(s) => starfish::pygame::draw::rect(&*s.surface, c, r, 0),
                Target::Active => bridge::with_session(vm, |t| starfish::pygame::draw::rect(t, c, r, 0))?,
            };
            Ok(())
        }

        /// rounded_rectangle(f, rect, radius, color)：垫底——圆角语义
        /// v1 未建（glam 无 SDF 圆角管线），降级为直角描边矩形。
        #[pyfunction]
        fn rounded_rectangle(
            f: PyObjectRef,
            rect: SfRect,
            _radius: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            rectangle(f, rect, color, vm)
        }

        /// rounded_box(f, rect, radius, color)：垫底——降级为填充矩形。
        #[pyfunction]
        fn rounded_box(
            f: PyObjectRef,
            rect: SfRect,
            _radius: i32,
            color: SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            box_(f, rect, color, vm)
        }
    }

    // ── mask：位面（碰撞检测/像素级遮挡；pygame.mask 语义）────────
    // 构建 = from_surface（影子快路径三端同步零回读；GPU 出生面原生
    // 走一次性回读，Web 侧同步面对 GPU 出生面不提供——from_surface_async
    // 承载，同 io 的 Web 分门先例）。
    #[pymodule]
    pub(crate) mod mask {
        use rustpython_derive::pyclass;
        use rustpython_vm::types::Constructor;
        use rustpython_vm::{
            Py, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine, builtins::PyType,
        };
        use starfish::pygame::mask::Mask as SfMask;

        use crate::rpy::args::SfPoint;

        /// pygame.mask.Mask——payload = starfish Mask（RefCell：位面
        /// 方法带 &mut；无 Debug → 手写 impl）。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.mask", name = "Mask", unhashable = true)]
        pub(crate) struct PyMask {
            pub(crate) inner: std::cell::RefCell<SfMask>,
        }

        impl std::fmt::Debug for PyMask {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let (w, h) = self.inner.borrow().size();
                f.write_str(&format!("Mask({}x{})", w, h))
            }
        }

        pub(crate) type PyMaskRef = PyRef<PyMask>;

        #[pyclass(with(Constructor))]
        impl PyMask {
            /// get_at(pos) -> bool：位读取。
            #[pymethod]
            fn get_at(&self, pos: SfPoint) -> bool {
                self.inner
                    .borrow()
                    .get_at(pos.0.max(0) as u32, pos.1.max(0) as u32)
            }

            /// set_at(pos, value=True)：位写入。
            #[pymethod]
            fn set_at(&self, args: crate::rpy::args::MaskSetAtArgs) {
                self.inner.borrow_mut().set_at(
                    args.pos.0.max(0) as u32,
                    args.pos.1.max(0) as u32,
                    args.value.unwrap_or(true),
                );
            }

            /// count() -> int：置位数。
            #[pymethod]
            fn count(&self) -> u32 {
                self.inner.borrow().count()
            }

            /// get_size() -> (w, h)。
            #[pymethod]
            fn get_size(&self) -> (u32, u32) {
                self.inner.borrow().size()
            }

            /// fill(value=True)：全位置位/清零。
            #[pymethod]
            fn fill(&self, args: crate::rpy::args::MaskFillArgs) {
                self.inner.borrow_mut().fill(args.value.unwrap_or(true));
            }

            /// invert()：全位取反。
            #[pymethod]
            fn invert(&self) {
                self.inner.borrow_mut().invert();
            }

            /// overlap(other, offset) -> (x, y) | None：首个重叠点
            /// （None = 无重叠；offset = other 相对本的偏移）。
            #[pymethod]
            fn overlap(
                &self,
                other: PyRef<PyMask>,
                offset: SfPoint,
            ) -> Option<(i32, i32)> {
                self.inner
                    .borrow()
                    .overlap(&other.inner.borrow(), (offset.0, offset.1))
            }

            /// overlap_area(other, offset) -> int：重叠位数量。
            #[pymethod]
            fn overlap_area(&self, other: PyRef<PyMask>, offset: SfPoint) -> u32 {
                self.inner
                    .borrow()
                    .overlap_area(&other.inner.borrow(), (offset.0, offset.1))
            }

            /// overlap_mask(other, offset) -> Mask：重叠位图。
            #[pymethod]
            fn overlap_mask(
                &self,
                other: PyRef<PyMask>,
                offset: SfPoint,
                vm: &VirtualMachine,
            ) -> PyRef<PyMask> {
                let m = self
                    .inner
                    .borrow()
                    .overlap_mask(&other.inner.borrow(), (offset.0, offset.1));
                PyMask { inner: std::cell::RefCell::new(m) }.into_ref(&vm.ctx)
            }

            /// scale((w, h)) -> Mask：最近邻缩放（新位面）。
            #[pymethod]
            fn scale(&self, size: SfPoint, vm: &VirtualMachine) -> PyRef<PyMask> {
                let m = self
                    .inner
                    .borrow()
                    .scale((size.0.max(0) as u32, size.1.max(0) as u32));
                PyMask { inner: std::cell::RefCell::new(m) }.into_ref(&vm.ctx)
            }

            /// to_surface() -> Surface：位面可视化（置位 = 白）。
            #[pymethod]
            fn to_surface(&self, vm: &VirtualMachine) -> PyRef<super::PySurface> {
                let s = self.inner.borrow().to_surface();
                super::PySurface { surface: std::sync::Arc::new(s) }.into_ref(&vm.ctx)
            }
        }

        impl Constructor for PyMask {
            type Args = crate::rpy::args::MaskArgs;

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                _vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self {
                    inner: std::cell::RefCell::new(SfMask::new(
                        (args.size.0.max(0) as u32, args.size.1.max(0) as u32),
                        args.fill.unwrap_or(false),
                    )),
                })
            }
        }

        /// from_surface(surface, threshold=127) -> Mask：离屏面 → 位面
        /// （alpha > threshold 置位）。影子快路径三端同步零回读；GPU
        /// 出生面的回读慢路径建议资产装载期构建、缓存复用（Web 端
        /// GPU 出生面走 from_surface_async 语义，本同步面对其返回错误）。
        #[pyfunction]
        fn from_surface(
            surface: PyRef<super::PySurface>,
            threshold: Option<u8>,
            vm: &VirtualMachine,
        ) -> PyResult<PyRef<PyMask>> {
            let m = SfMask::from_surface(&surface.surface, threshold.unwrap_or(127))
                .map_err(|e| vm.new_value_error(format!("mask.from_surface 失败: {e:?}")))?;
            Ok(PyMask { inner: std::cell::RefCell::new(m) }.into_ref(&vm.ctx))
        }
    }

    // ── permission：运行时权限（三平台语义差异见引擎模块文档）──────
    // 桌面直通 / Web 委派 / Android：INTERNET 瞬时，MICROPHONE 阻塞
    // 等待授权框——**绑定面同语义透传**（调用方自行决定申请时机）。
    #[pymodule]
    pub(crate) mod permission {
        use rustpython_vm::{PyResult, VirtualMachine};
        use starfish::pygame::permission::{self as sf_perm, Permission};

        /// 权限标识：pygame.permission.MICROPHONE / INTERNET
        #[pyattr]
        pub(crate) const MICROPHONE: i32 = 0;
        #[pyattr]
        pub(crate) const INTERNET: i32 = 1;

        fn map_perm(p: i32, vm: &VirtualMachine) -> PyResult<Permission> {
            match p {
                MICROPHONE => Ok(Permission::Microphone),
                INTERNET => Ok(Permission::Internet),
                _ => Err(vm.new_value_error(format!(
                    "未知权限标识 {p}（可用：pygame.permission.MICROPHONE / INTERNET）"
                ))),
            }
        }

        /// ensure(permission) -> bool：同步申请（Android MICROPHONE 会
        /// 弹系统授权框**阻塞等待**——自动化冒烟勿触碰；INTERNET 瞬时）。
        #[pyfunction]
        fn ensure(p: i32, vm: &VirtualMachine) -> PyResult<bool> {
            Ok(sf_perm::ensure(map_perm(p, vm)?))
        }
    }

    // ── video：视频解码（六平台硬解；接口面——真实播放的呈现集成
    // 需引擎侧纹理桥，v1 接口可调即达标）──────────────────────────
    #[pymodule]
    pub(crate) mod video {
        use rustpython_derive::pyclass;
        use rustpython_vm::types::Constructor;
        use rustpython_vm::{
            Py, PyPayload, PyRef, PyResult, VirtualMachine,
            builtins::{PyStrRef, PyType},
        };
        use starfish::pygame::video::{VideoSystem as SfVideoSystem, Video as SfVideo};

        /// pygame.video.VideoSystem——解码系统入口（set_mode 后创建）。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.video", name = "VideoSystem", unhashable = true)]
        pub(crate) struct PyVideoSystem {
            pub(crate) inner: SfVideoSystem,
        }

        impl std::fmt::Debug for PyVideoSystem {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("VideoSystem(<decoder>)")
            }
        }

        #[pyclass(with(Constructor))]
        impl PyVideoSystem {
            /// open(path) -> Video：打开媒体文件（缺失/不支持格式抛 ValueError）。
            #[pymethod]
            fn open(&self, path: PyStrRef, vm: &VirtualMachine) -> PyResult<PyRef<PyVideo>> {
                let p = path
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("路径含非 UTF-8 字符"))?;
                let v = self
                    .inner
                    .open(p)
                    .map_err(|e| vm.new_value_error(format!("video.open 失败: {e:?}")))?;
                Ok(PyVideo { inner: std::cell::RefCell::new(v) }.into_ref(&vm.ctx))
            }
        }

        impl Constructor for PyVideoSystem {
            type Args = ();

            fn py_new(
                _cls: &Py<PyType>,
                _args: Self::Args,
                _vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self { inner: SfVideoSystem::new() })
            }
        }

        /// pygame.video.Video——已打开的媒体（update 驱动解码推进）。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.video", name = "Video", unhashable = true)]
        pub(crate) struct PyVideo {
            pub(crate) inner: std::cell::RefCell<SfVideo>,
        }

        impl std::fmt::Debug for PyVideo {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let (w, h) = self.inner.borrow().size();
                f.write_str(&format!("Video({}x{})", w, h))
            }
        }

        #[pyclass]
        impl PyVideo {
            /// update(dt_seconds)：推进解码（dt = 距上次调用的秒数；
            /// 解码错误抛 ValueError）。
            #[pymethod]
            fn update(&self, dt: f64, vm: &VirtualMachine) -> PyResult<()> {
                self.inner
                    .borrow_mut()
                    .update(std::time::Duration::from_secs_f64(dt.max(0.0)))
                    .map_err(|e| vm.new_value_error(format!("video.update 失败: {e:?}")))
            }

            /// size() -> (w, h)：视频原始尺寸。
            #[pymethod]
            fn size(&self) -> (u32, u32) {
                self.inner.borrow().size()
            }

            /// position() -> float：当前播放位置（秒）。
            #[pymethod]
            fn position(&self) -> f64 {
                self.inner.borrow().position().as_secs_f64()
            }

            /// ended() -> bool：是否播放到尾。
            #[pymethod]
            fn ended(&self) -> bool {
                self.inner.borrow().ended()
            }

            /// has_audio() -> bool：是否含音轨。
            #[pymethod]
            fn has_audio(&self) -> bool {
                self.inner.borrow().has_audio()
            }

            /// set_video_enabled(enabled)：视频轨开关（纯音轨模式）。
            #[pymethod]
            fn set_video_enabled(&self, enabled: bool) {
                self.inner.borrow_mut().set_video_enabled(enabled)
            }

            /// video_enabled() -> bool。
            #[pymethod]
            fn video_enabled(&self) -> bool {
                self.inner.borrow().video_enabled()
            }

            /// set_audio_volume(v)：音量 0.0..1.0。
            #[pymethod]
            fn set_audio_volume(&self, v: f32) {
                self.inner.borrow().set_audio_volume(v)
            }

            /// set_muted(m)：静音开关。
            #[pymethod]
            fn set_muted(&self, m: bool) {
                self.inner.borrow().set_muted(m)
            }
        }
    }

    // ── targets：MRT 会话（`with pygame.targets([a, b]) as t:`——一次
    //    pass 镜像写 N≥2 目标；兼容层模块架构 §3.1 预言形态落地）────
    // 方法面 = 会话绘制核心集（fill/rect/circle/line/polygon/ellipse/
    // arc/blit）；目标 Arc 由 bridge 保活至 __exit__（render_targets_arc
    // 契约）。块内绘制经 with_mrt_session 路由。
    /// pygame.targets(surfaces) -> TargetsSession：MRT 会话入口
    /// （N≥2 个 Surface 一次 pass 镜像写入）。
    #[allow(non_snake_case)]
    #[pyfunction]
    fn targets(surfaces: Vec<PyRef<PySurface>>) -> PyTargetsSession {
        PyTargetsSession {
            surfaces: surfaces.into_iter().map(|s| s.surface.clone()).collect(),
        }
    }

    #[derive(PyPayload)]
    #[pyclass(no_attr, module = false, name = "TargetsSession", unhashable = true)]
    pub(crate) struct PyTargetsSession {
        pub(crate) surfaces: Vec<std::sync::Arc<SfSurface>>,
    }

    impl std::fmt::Debug for PyTargetsSession {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&format!("TargetsSession(×{} 目标)", self.surfaces.len()))
        }
    }

    #[pyclass]
    impl PyTargetsSession {
        #[pymethod]
        fn __enter__(zelf: PyRef<Self>) -> PyRef<Self> {
            bridge::mrt_begin(&zelf.surfaces);
            zelf
        }

        /// 异常不吞（返回 False 交回宿主壳打印收口）。
        #[pymethod]
        fn __exit__(&self, _exc_info: FuncArgs) -> bool {
            bridge::mrt_end();
            false
        }

        #[pymethod]
        fn fill(&self, args: args::FillArgs, vm: &VirtualMachine) -> PyResult<()> {
            let rgba = args.color.0;
            bridge::with_mrt_session(vm, |p| {
                use starfish::pygame::render::DrawTarget;
                p.fill(rgba);
            })
        }

        #[pymethod]
        fn rect(&self, args: args::DrawRectArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                starfish::pygame::draw::rect(
                    p,
                    bridge::color_of(args.color),
                    starfish::pygame::Rect::new(args.rect.x, args.rect.y, args.rect.w, args.rect.h),
                    args.width.unwrap_or(0).max(0) as u32,
                );
            })
        }

        #[pymethod]
        fn circle(&self, args: args::DrawCircleArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                starfish::pygame::draw::circle(
                    p,
                    bridge::color_of(args.color),
                    (args.center.0, args.center.1),
                    args.radius.max(0) as u32,
                    args.width.unwrap_or(0).max(0) as u32,
                );
            })
        }

        #[pymethod]
        fn line(&self, args: args::DrawLineArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                starfish::pygame::draw::line(
                    p,
                    bridge::color_of(args.color),
                    (args.start.0, args.start.1),
                    (args.end.0, args.end.1),
                    args.width.unwrap_or(1).max(0) as u32,
                );
            })
        }

        #[pymethod]
        fn polygon(&self, args: args::DrawPolygonArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                starfish::pygame::draw::polygon(
                    p,
                    bridge::color_of(args.color),
                    &args.points.0,
                    args.width.unwrap_or(0).max(0) as u32,
                );
            })
        }

        #[pymethod]
        fn ellipse(&self, args: args::DrawEllipseArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                starfish::pygame::draw::ellipse(
                    p,
                    bridge::color_of(args.color),
                    starfish::pygame::Rect::new(args.rect.x, args.rect.y, args.rect.w, args.rect.h),
                    args.width.unwrap_or(0).max(0) as u32,
                );
            })
        }

        #[pymethod]
        fn arc(&self, args: args::DrawArcArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                starfish::pygame::draw::arc(
                    p,
                    bridge::color_of(args.color),
                    starfish::pygame::Rect::new(args.rect.x, args.rect.y, args.rect.w, args.rect.h),
                    args.start_angle.into_float() as f32,
                    args.stop_angle.into_float() as f32,
                    args.width.unwrap_or(1).max(0) as u32,
                );
            })
        }

        /// blit(src, dest) -> Rect：Surface 贴入 MRT 会话。
        #[pymethod]
        fn blit(
            &self,
            src: PyRef<PySurface>,
            dest: args::SfPoint,
            vm: &VirtualMachine,
        ) -> PyResult<PyObjectRef> {
            bridge::with_mrt_session(vm, |p| {
                use starfish::pygame::render::DrawTarget;
                p.blit(&src.surface, (dest.0, dest.1))
            })
            .map(|r| PyRect(Cell::new(r)).into_ref(&vm.ctx).into())
        }

        /// blit_area(src, dest, area, color=None) -> Rect：UV 子区域贴图
        /// （with_batch push_quad_uv 直通）。
        #[pymethod]
        fn blit_area(&self, args: args::BlitAreaArgs, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let r = bridge::with_mrt_session(vm, |p| {
                bridge::blit_area(p, &args.src, args.dest, args.area, args.color, vm)
            })??;
            Ok(PyRect(Cell::new(r)).into_ref(&vm.ctx).into())
        }

        /// quad(texture, corners, uvs, color)：任意四角贴图（with_batch
        /// push_quad_corners 直通）。
        #[pymethod]
        fn quad(&self, args: args::QuadCornersArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| bridge::quad_corners(p, args, vm))?;
            Ok(())
        }
    }

    // ── sprite：精灵/组（Godot 式池化组 + 世代 id 的 pyclass 桥）──────
    // 设计：Python 侧 Sprite = 配置对象（表面 + 链式参数），Group.add
    // 时物化为 starfish Sprite 并把返回的世代 id 回写进配置对象（Copy
    // 纯值，跨 GC 边界零生命周期负担）；此后该 Sprite 即"绑定到组"的
    // 句柄（alive/position/move_by 均经 id 路由）。draw 路由活跃会话
    // （单形态定约：with screen.render() as r: 块内 g.draw()）。
    #[pymodule]
    mod sprite {
        use rustpython_derive::pyclass;
        use rustpython_vm::types::Constructor;
        use rustpython_vm::{
            Py, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine,
            builtins::{PyStrRef, PyType},
        };
        use starfish::pygame::sprite as sf_sprite;

        use crate::rpy::args::SfPoint;

        /// 组条目的完整规格（add 时一次性物化）
        struct SpriteSpec {
            at: (i32, i32),
            tag: Option<String>,
            z: i32,
            rotation: f32,
            scale: f32,
            alpha: Option<f32>,
            visible: bool,
            fps: f32,
        }

        /// pygame.sprite.Sprite——配置对象；Group.add 后成为组句柄。
        /// 单帧 = Surface；多帧 = [Surface, ...]（帧动画，fps 由
        /// `set_fps` 定，缺省 4）。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.sprite", name = "Sprite", unhashable = true)]
        pub(crate) struct PySprite {
            pub(crate) surfaces: Vec<std::sync::Arc<super::SfSurface>>,
            pub(crate) spec: std::cell::RefCell<SpriteSpec>,
            pub(crate) id: std::cell::Cell<Option<sf_sprite::SpriteId>>,
        }

        impl std::fmt::Debug for PySprite {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&format!("Sprite({} 帧, x{} 帧)", self.surfaces.len(), self.spec.borrow().at.0))
            }
        }

        impl Constructor for PySprite {
            type Args = (PyObjectRef,);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                vm: &VirtualMachine,
            ) -> PyResult<Self> {
                let (img,) = args;
                // Surface 实例或 [Surface, ...] 列表双形态
                let surfaces: Vec<std::sync::Arc<super::SfSurface>> = if let Some(s) =
                    img.downcast_ref::<super::PySurface>()
                {
                    vec![s.surface.clone()]
                } else {
                    vm.extract_elements_with(&img, |o: PyObjectRef| {
                        o.downcast_ref::<super::PySurface>()
                            .map(|s| s.surface.clone())
                            .ok_or_else(|| {
                                vm.new_type_error("Sprite 参数须为 Surface 或 Surface 列表")
                            })
                    })?
                };
                if surfaces.is_empty() {
                    return Err(vm.new_value_error("Sprite 至少需要一帧 Surface"));
                }
                Ok(Self {
                    surfaces,
                    spec: std::cell::RefCell::new(SpriteSpec {
                        at: (0, 0),
                        tag: None,
                        z: 0,
                        rotation: 0.0,
                        scale: 1.0,
                        alpha: None,
                        visible: true,
                        fps: 4.0,
                    }),
                    id: std::cell::Cell::new(None),
                })
            }
        }

        #[pyclass(with(Constructor))]
        impl PySprite {
            /// at(x, y) -> self：初始位置（链式）
            #[pymethod]
            fn at(zelf: PyRef<Self>, x: i32, y: i32) -> PyRef<Self> {
                zelf.spec.borrow_mut().at = (x, y);
                zelf
            }

            /// tag(name) -> self：分组标签（链式；with_tag 查询用）
            #[pymethod]
            fn tag(zelf: PyRef<Self>, tag: PyStrRef, vm: &VirtualMachine) -> PyResult<PyRef<Self>> {
                let t = tag
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("tag 含非 UTF-8 字符"))?
                    .to_owned();
                zelf.spec.borrow_mut().tag = Some(t);
                Ok(zelf)
            }

            /// z(v) -> self：层级（(z, y) 排序会话绘制）
            #[pymethod]
            fn z(zelf: PyRef<Self>, v: i32) -> PyRef<Self> {
                zelf.spec.borrow_mut().z = v;
                zelf
            }

            /// rotation(deg) -> self：旋转角（度）
            #[pymethod]
            fn rotation(zelf: PyRef<Self>, deg: f32) -> PyRef<Self> {
                zelf.spec.borrow_mut().rotation = deg;
                zelf
            }

            /// scale(f) -> self：等比缩放
            #[pymethod]
            fn scale(zelf: PyRef<Self>, f: f32) -> PyRef<Self> {
                zelf.spec.borrow_mut().scale = f;
                zelf
            }

            /// alpha(a) -> self：表面级半透明（0.0..1.0）
            #[pymethod]
            fn alpha(zelf: PyRef<Self>, a: f32) -> PyRef<Self> {
                zelf.spec.borrow_mut().alpha = Some(a.clamp(0.0, 1.0));
                zelf
            }

            /// visible(b) -> self：可见性
            #[pymethod]
            fn visible(zelf: PyRef<Self>, b: bool) -> PyRef<Self> {
                zelf.spec.borrow_mut().visible = b;
                zelf
            }

            /// set_fps(f)：多帧动画帧率（缺省 4）
            #[pymethod]
            fn set_fps(&self, f: f32) {
                self.spec.borrow_mut().fps = f.max(0.1);
            }

            /// alive() -> bool：是否已在组内存活（未 add = False）
            #[pymethod]
            fn alive(&self) -> bool {
                self.id.get().is_some()
            }
        }

        /// pygame.sprite.Group——池化组（世代 id；remove 后旧句柄自动失效）。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.sprite", name = "Group", unhashable = true)]
        pub(crate) struct PyGroup {
            pub(crate) inner: std::cell::RefCell<sf_sprite::Group>,
        }

        impl std::fmt::Debug for PyGroup {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("Group(<pooled>)")
            }
        }

        fn spec_to_sprite(sprite: &PySprite, vm: &VirtualMachine) -> PyResult<sf_sprite::Sprite> {
            let spec = sprite.spec.borrow();
            let mut it = if sprite.surfaces.len() >= 2 {
                let refs: Vec<&super::SfSurface> =
                    sprite.surfaces.iter().map(|s| s.as_ref()).collect();
                sf_sprite::Sprite::with_frames(&refs, spec.fps)
            } else {
                sf_sprite::Sprite::new(&sprite.surfaces[0])
            };
            it = it.at(spec.at.0, spec.at.1).z(spec.z).scale(spec.scale).visible(spec.visible);
            if spec.rotation != 0.0 {
                it = it.rotated(spec.rotation);
            }
            if let Some(a) = spec.alpha {
                it = it.alpha(a);
            }
            if let Some(t) = &spec.tag {
                let leaked: &'static str = t.clone().leak();
                it = it.tag(leaked);
            }
            let _ = vm;
            Ok(it)
        }

        fn id_of(sprite: &PySprite, vm: &VirtualMachine) -> PyResult<sf_sprite::SpriteId> {
            sprite.id.get().ok_or_else(|| {
                vm.new_value_error("Sprite 尚未加入任何 Group（先 group.add(sprite)）")
            })
        }

        #[pyclass(with(Constructor))]
        impl PyGroup {

            /// add(sprite) -> sprite：物化配置入组并回写世代 id
            /// （返回原对象以便链式/即取即用）。
            #[pymethod]
            fn add(&self, sprite: PyRef<PySprite>, vm: &VirtualMachine) -> PyResult<PyRef<PySprite>> {
                let it = spec_to_sprite(&sprite, vm)?;
                let id = self.inner.borrow_mut().insert(it);
                sprite.id.set(Some(id));
                Ok(sprite)
            }

            /// remove(sprite) -> bool：出组（世代 id 失效，旧句柄安全）。
            #[pymethod]
            fn remove(&self, sprite: PyRef<PySprite>, vm: &VirtualMachine) -> PyResult<bool> {
                let id = id_of(&sprite, vm)?;
                let removed = self.inner.borrow_mut().remove(id);
                if removed {
                    sprite.id.set(None);
                }
                Ok(removed)
            }

            /// alive(sprite) -> bool：句柄是否存活。
            #[pymethod]
            fn alive(&self, sprite: PyRef<PySprite>, vm: &VirtualMachine) -> PyResult<bool> {
                match sprite.id.get() {
                    Some(id) => Ok(self.inner.borrow().is_alive(id)),
                    None => Ok(false),
                }
            }

            /// count() -> int：组内存活精灵数。
            #[pymethod]
            fn count(&self) -> usize {
                self.inner.borrow().count()
            }

            /// position(sprite) -> (x, y) | None。
            #[pymethod]
            fn position(
                &self,
                sprite: PyRef<PySprite>,
                vm: &VirtualMachine,
            ) -> PyResult<Option<(i32, i32)>> {
                let id = id_of(&sprite, vm)?;
                Ok(self.inner.borrow().position(id))
            }

            /// move_by(sprite, dx, dy)：位移。
            #[pymethod]
            fn move_by(&self, sprite: PyRef<PySprite>, dx: i32, dy: i32, vm: &VirtualMachine) -> PyResult<()> {
                let id = id_of(&sprite, vm)?;
                self.inner.borrow_mut().move_by(id, dx, dy);
                Ok(())
            }

            /// set_position(sprite, x, y)。
            #[pymethod]
            fn set_position(&self, sprite: PyRef<PySprite>, x: i32, y: i32, vm: &VirtualMachine) -> PyResult<()> {
                let id = id_of(&sprite, vm)?;
                self.inner.borrow_mut().set_position(id, x, y);
                Ok(())
            }

            /// set_z(sprite, v)：层级。
            #[pymethod]
            fn set_z(&self, sprite: PyRef<PySprite>, v: i32, vm: &VirtualMachine) -> PyResult<()> {
                let id = id_of(&sprite, vm)?;
                self.inner.borrow_mut().set_z(id, v);
                Ok(())
            }

            /// set_rotation(sprite, deg)。
            #[pymethod]
            fn set_rotation(&self, sprite: PyRef<PySprite>, deg: f32, vm: &VirtualMachine) -> PyResult<()> {
                let id = id_of(&sprite, vm)?;
                self.inner.borrow_mut().set_rotation(id, deg);
                Ok(())
            }

            /// set_scale(sprite, f)。
            #[pymethod]
            fn set_scale(&self, sprite: PyRef<PySprite>, f: f32, vm: &VirtualMachine) -> PyResult<()> {
                let id = id_of(&sprite, vm)?;
                self.inner.borrow_mut().set_scale(id, f);
                Ok(())
            }

            /// set_visible(sprite, b)。
            #[pymethod]
            fn set_visible(&self, sprite: PyRef<PySprite>, b: bool, vm: &VirtualMachine) -> PyResult<()> {
                let id = id_of(&sprite, vm)?;
                self.inner.borrow_mut().set_visible(id, b);
                Ok(())
            }

            /// with_tag(tag) -> [Sprite]：标签过滤（返回存活句柄列表）。
            #[pymethod]
            fn with_tag(&self, tag: PyStrRef, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
                let t = tag
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("tag 含非 UTF-8 字符"))?;
                let _ = vm;
                // v1：id 级过滤无法反查 Python 对象——返回数量（句柄反查
                // 属对象表设计，随 v2 资源句柄表立项）
                let n = self.inner.borrow().with_tag(t).len();
                Ok(vm.ctx.new_int(n as i64).into())
            }

            /// tick(dt_seconds)：推进帧动画/时序。
            #[pymethod]
            fn tick(&self, dt: f32) {
                self.inner.borrow_mut().tick(dt);
            }

            /// draw()：全组绘制到**活跃会话**（单形态定约——须在
            /// `with screen.render() as r:` 块内调用）。
            #[pymethod]
            fn draw(&self, vm: &VirtualMachine) -> PyResult<()> {
                crate::rpy::bridge::with_session(vm, |p| self.inner.borrow().draw(p))
            }
        }

        impl Constructor for PyGroup {
            type Args = ();

            fn py_new(
                _cls: &Py<PyType>,
                _args: Self::Args,
                _vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self { inner: std::cell::RefCell::new(sf_sprite::Group::new()) })
            }
        }
    }

}
