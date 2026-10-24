//! pygame 父模块壳（2026-10-24 整理批次）：承载顶层类型（Surface/
//! SurfaceSession/Screen/Session/Color——#[pyclass] struct+impl
//! 必须同宿 pymodule）+ init/quit 占位 + targets MRT 会话入口；子模块
//! 在各 mods/*.rs，由 registry 按依赖序挂载（event 先于 key）。
//! Rect 类型已退役（2026-10-24 定约：GPU 全帧 present 无脏矩形语义，
//! 碰撞/触发器归物理引擎）——绘制面无返回值，区域几何用 IRect 载体。

use rustpython_derive::pymodule;
#[pymodule(name = "pygame")]
pub mod pygame {
    use std::cell::Cell;
    use crate::rpy::mods::font::font;
    use crate::rpy::{bridge, args};
    use starfish_pygame::render::{DrawTarget, Surface as SfSurface};
    use rustpython_derive::{pyclass, FromArgs};
    use rustpython_vm::types::Constructor;
    use rustpython_vm::convert::TryFromObject;
    use rustpython_vm::{
        PyObjectRef, Py, PyObject, PyPayload, PyRef, PyResult, VirtualMachine,
        function::{FuncArgs, PyComparisonValue},
        types::{Comparable, PyComparisonOp, Representable},
        builtins::{PyModule, PyStr, PyStrRef, PyType},
    };
    use starfish_pygame::color::Color as EngineColor;
    use crate::rpy::args::{SfColor, SfPoint, SfRect};
    use crate::rpy::mods::event::PyKey;
    pub(crate) const DEFAULT_FONT: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/fonts/Antonio-Regular.ttf"
    ));

    // ── Surface：starfish Surface 直通句柄（批次 F）─────────────────
    // payload = owned starfish Surface（无 Clone/Debug/生命周期参数；
    // 全 API &self 内部可变性）。构造/使用须在 display init 后（GPU 槽）。
    #[derive(PyPayload)]
    #[pyattr]
    #[pyclass(name = "Surface", unhashable = true)]
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

    impl Constructor for PySurface {
        /// Surface((w, h), depth_buffer=False)：GPU 需已 init（构造/使用须在
        /// display init 后——与真 pygame 的差异，文档标注）。depth_buffer=
        /// 深度缓冲（绑定定约 §九.2——`depth=` 为真 pygame bpp 语义，禁用）。
        type Args = crate::rpy::args::SurfaceArgs;

        fn py_new(
            _cls: &Py<PyType>,
            args: Self::Args,
            _vm: &VirtualMachine,
        ) -> PyResult<Self> {
            let size = (args.size.0.max(0) as u32, args.size.1.max(0) as u32);
            let surface = if args.depth_buffer.unwrap_or(false) {
                starfish_pygame::render::Surface::with_depth(size)
            } else {
                SfSurface::new(size)
            };
            Ok(Self { surface: std::sync::Arc::new(surface) })
        }
    }

    #[pyclass(with(Constructor, Representable))]
    impl PySurface {
        /// render() -> SurfaceSession：with 会话入口（离屏绘制同 Screen 语法）
        #[pymethod]
        fn render(&self, vm: &VirtualMachine) -> PyResult<PyRef<PySurfaceSession>> {
            Ok(PySurfaceSession {
                surface: self.surface.clone(),
            }
            .into_ref(&vm.ctx))
        }

        /// has_depth() -> bool：是否带深度缓冲（构造期 `depth_buffer=True`
        /// 定格——`render_depth()` 深度会话的前置）
        #[pymethod]
        fn has_depth(&self) -> bool {
            self.surface.has_depth()
        }

        /// set_clip(rect | None)：离屏会话裁剪矩形（后续绘制生效）
        #[pymethod]
        fn set_clip(&self, rect: Option<SfRect>, _vm: &VirtualMachine) {
            self.surface.set_clip(rect.map(|r| {
                [r.x, r.y, r.w, r.h]
            }));
        }

        #[pymethod]
        fn get_size(&self) -> (u32, u32) {
            self.surface.size()
        }

        /// set_alpha(value)：表面级透明度（0..255；blit 时乘进顶点色 alpha
        /// ——批次 L。真 pygame 按源像素 alpha 位与全局值协作，本层恒全局乘）
        #[pymethod]
        fn set_alpha(&self, value: i32) {
            self.surface.set_alpha(value.clamp(0, 255) as u8);
        }

        /// get_alpha() -> int：表面级透明度
        #[pymethod]
        fn get_alpha(&self) -> u8 {
            self.surface.alpha()
        }

        /// fill(color)：整面填充（DrawTarget 直通；无返回值——GPU 全帧
        /// present 无脏矩形概念，Rect 门面已退役）
        #[pymethod]
        fn fill(&self, color: SfColor, vm: &VirtualMachine) -> PyResult<()> {
            DrawTarget::fill(&*self.surface, [color.0[0], color.0[1], color.0[2], color.0[3]]);
            Ok(())
        }

        /// blit(src, dest)：Surface 贴图（self-blit 会 panic——
        /// starfish assert_not_self，文档标注）
        #[pymethod]
        fn blit(
            &self,
            src: PyRef<PySurface>,
            dest: SfPoint,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            DrawTarget::blit(&*self.surface, &src.surface, (dest.0, dest.1));
            Ok(())
        }
    }

    impl Comparable for PySurface {
        fn cmp(
            zelf: &Py<Self>,
            other: &PyObject,
            op: PyComparisonOp,
            _vm: &VirtualMachine,
        ) -> PyResult<PyComparisonValue> {
            if op == PyComparisonOp::Eq {
                if let Some(o) = other.downcast_ref::<PySurface>() {
                    return Ok(PyComparisonValue::Implemented(
                        DrawTarget::size(&*zelf.surface) == DrawTarget::size(&*o.surface)
                    ));
                }
            }
            Ok(PyComparisonValue::NotImplemented)
        }
    }

    impl Representable for PySurface {
        fn repr(zelf: &Py<Self>, vm: &VirtualMachine) -> PyResult<PyRef<PyStr>> {
            let (w, h) = DrawTarget::size(&*zelf.surface);
            Ok(vm.ctx.new_str(format!("<Surface({}x{})>", w, h)))
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
        fn fill(&self, color: crate::rpy::args::SfColor, vm: &VirtualMachine) -> PyResult<()> {
            use starfish_pygame::render::DrawTarget;
            DrawTarget::fill(&*self.surface, [color.0[0], color.0[1], color.0[2], color.0[3]]);
            let _ = vm;
            Ok(())
        }

        #[pymethod]
        fn rect(&self, args: args::DrawRectArgs, vm: &VirtualMachine) -> PyResult<()> {
            starfish_pygame::draw::rect(
                &*self.surface,
                args.color.0,
                [args.rect.x, args.rect.y, args.rect.w, args.rect.h],
                args.width.unwrap_or(0).max(0) as u32,
            );
            Ok(())
        }

        #[pymethod]
        fn circle(&self, args: args::DrawCircleArgs, vm: &VirtualMachine) -> PyResult<()> {
            starfish_pygame::draw::circle(
                &*self.surface,
                args.color.0,
                (args.center.0, args.center.1),
                args.radius.max(0) as u32,
                args.width.unwrap_or(0).max(0) as u32,
            );
            Ok(())
        }

        #[pymethod]
        fn line(&self, args: args::DrawLineArgs, vm: &VirtualMachine) -> PyResult<()> {
            starfish_pygame::draw::line(
                &*self.surface,
                args.color.0,
                (args.start.0, args.start.1),
                (args.end.0, args.end.1),
                args.width.unwrap_or(1).max(1) as u32,
            );
            Ok(())
        }

        #[pymethod]
        fn polygon(&self, args: args::DrawPolygonArgs, vm: &VirtualMachine) -> PyResult<()> {
            starfish_pygame::draw::polygon(
                &*self.surface,
                args.color.0,
                &args.points.0,
                args.width.unwrap_or(0).max(0) as u32,
            );
            Ok(())
        }

        #[pymethod]
        fn ellipse(&self, args: args::DrawEllipseArgs, vm: &VirtualMachine) -> PyResult<()> {
            starfish_pygame::draw::ellipse(
                &*self.surface,
                args.color.0,
                [args.rect.x, args.rect.y, args.rect.w, args.rect.h],
                args.width.unwrap_or(0).max(0) as u32,
            );
            Ok(())
        }

        #[pymethod]
        fn arc(&self, args: args::DrawArcArgs, vm: &VirtualMachine) -> PyResult<()> {
            starfish_pygame::draw::arc(
                &*self.surface,
                args.color.0,
                [args.rect.x, args.rect.y, args.rect.w, args.rect.h],
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
        ) -> PyResult<()> {
            use starfish_pygame::render::DrawTarget;
            DrawTarget::blit(&*self.surface, &src.surface, (dest.0, dest.1));
            let _ = vm;
            Ok(())
        }

        /// blit_area(src, dest, area, color=None)：UV 子区域贴图
        /// （with_batch push_quad_uv 直通——源面像素坐标子区域一次四边形）
        #[pymethod]
        fn blit_area(&self, args: args::BlitAreaArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::blit_area(&*self.surface, &args.src, args.dest, args.area, args.color, vm)
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

        /// text(font, text, dest, color)：会话内直接绘制文字
        /// （目标 = 本 Surface；font.render + blit 融合，批次 F+）
        #[pymethod]
        fn text(
            &self,
            font: PyRef<font::PyFont>,
            text: PyStrRef,
            dest: args::SfPoint,
            color: crate::rpy::args::SfColor,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            use rustpython_vm::convert::TryFromObject;
            use starfish_pygame::render::DrawTarget;
            let t = text
                .to_str()
                .ok_or_else(|| vm.new_value_error("文本含非 UTF-8 字符"))?;
            let rendered = font
                .inner
                .render(t, true, [color.0[0], color.0[1], color.0[2], color.0[3]]);
            DrawTarget::blit(&*self.surface, &rendered, (dest.0, dest.1));
            Ok(())
        }
    }
    // init/quit 占位函数在本 pymodule 壳尾部（#[pyfunction] 需 pymodule
    // 宏承载）。


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
            starfish_pygame::render::session::begin_screen();
            Ok(PyScreenSession.into_ref(&vm.ctx))
        }

        /// get_size() -> (w, h)：当前窗口/渲染表面尺寸（Web 响应式
        /// canvas 下跟随页面布局变化——脚本查询用）
        #[pymethod]
        fn get_size(&self) -> (u32, u32) {
            starfish_pygame::display::get_screen().size()
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
            starfish_pygame::render::session::end_screen();
            false
        }

        /// draw_ui(panel)（feature "gui"）：egui 面板构建 + 会话排程——
        /// "画到哪个目标"由会话对象表达。panel = pygame.egui.UiBase 子类
        /// 实例（OOP 门面，须实现 draw(self, ui)）；每帧一次；落屏在
        /// flip 的叠加阶段（恒在游戏内容之上）。
        #[cfg(feature = "gui")]
        #[pymethod]
        fn draw_ui(&self, panel: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
            crate::rpy::pyegui::draw_ui(vm, panel)
        }

        /// draw_debug(renderer, sink)（feature "physics2d"）：rapier 调试
        /// 线会话排程（与 draw_ui 同一渲染行为——会话内调用、恒在游戏
        /// 内容之上）；renderer/sink = pygame.rapier 对象。
        #[cfg(feature = "physics2d")]
        #[pymethod]
        fn draw_debug(
            &self,
            renderer: PyRef<crate::rpy::mods::rapier::PyDebugLinesRenderer>,
            sink: PyRef<crate::rpy::mods::rapier::PyLineSink>,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            crate::rpy::pyrapier::draw_debug(renderer, sink, vm)
        }

        #[pymethod]
        fn fill(&self, args: FillArgs, _vm: &VirtualMachine) -> PyResult<()> {
            // Screen 例外：全局屏填充不经会话（get_screen 进程唯一）
            starfish_pygame::display::get_screen().fill(args.color.0);
            Ok(())
        }

        #[pymethod]
        fn rect(&self, args: args::DrawRectArgs, vm: &VirtualMachine) -> PyResult<()> {
            let width = args.width.unwrap_or(0).max(0) as u32;
            bridge::with_session(vm, |t| {
                starfish_pygame::draw::rect(
                    t,
                    args.color.0,
                    [args.rect.x, args.rect.y, args.rect.w, args.rect.h],
                    width,
                );
            })
        }

        #[pymethod]
        fn circle(&self, args: args::DrawCircleArgs, vm: &VirtualMachine) -> PyResult<()> {
            let radius = args.radius.max(0) as u32;
            let width = args.width.unwrap_or(0).max(0) as u32;
            bridge::with_session(vm, |t| {
                starfish_pygame::draw::circle(
                    t,
                    args.color.0,
                    (args.center.0, args.center.1),
                    radius,
                    width,
                );
            })
        }

        #[pymethod]
        fn line(&self, args: args::DrawLineArgs, vm: &VirtualMachine) -> PyResult<()> {
            let width = args.width.unwrap_or(1).max(1) as u32;
            bridge::with_session(vm, |t| {
                starfish_pygame::draw::line(
                    t,
                    args.color.0,
                    (args.start.0, args.start.1),
                    (args.end.0, args.end.1),
                    width,
                );
            })
        }

        #[pymethod]
        fn polygon(&self, args: args::DrawPolygonArgs, vm: &VirtualMachine) -> PyResult<()> {
            let width = args.width.unwrap_or(0).max(0) as u32;
            bridge::with_session(vm, |t| {
                starfish_pygame::draw::polygon(t, args.color.0, &args.points.0, width);
            })
        }

        #[pymethod]
        fn ellipse(&self, args: args::DrawEllipseArgs, vm: &VirtualMachine) -> PyResult<()> {
            let width = args.width.unwrap_or(0).max(0) as u32;
            bridge::with_session(vm, |t| {
                starfish_pygame::draw::ellipse(
                    t,
                    args.color.0,
                    [args.rect.x, args.rect.y, args.rect.w, args.rect.h],
                    width,
                );
            })
        }

        #[pymethod]
        fn arc(&self, args: args::DrawArcArgs, vm: &VirtualMachine) -> PyResult<()> {
            let width = args.width.unwrap_or(1).max(1) as u32;
            bridge::with_session(vm, |t| {
                starfish_pygame::draw::arc(
                    t,
                    args.color.0,
                    [args.rect.x, args.rect.y, args.rect.w, args.rect.h],
                    args.start_angle.into_float() as f32,
                    args.stop_angle.into_float() as f32,
                    width,
                );
            })
        }

        /// blit(src, dest)：Surface 贴入会话（批次 F）
        #[pymethod]
        fn blit(&self, src: PyRef<PySurface>, dest: args::SfPoint, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_session(vm, |t| t.blit(&src.surface, (dest.0, dest.1)))
        }

        /// blit_area(src, dest, area, color=None)：UV 子区域贴图
        /// （with_batch push_quad_uv 直通——源面像素坐标子区域一次四边形）
        #[pymethod]
        fn blit_area(&self, args: args::BlitAreaArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_session(vm, |p| {
                bridge::blit_area(p, &args.src, args.dest, args.area, args.color, vm)
            })?
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

        /// text(font, text, dest, color)：font.render + 会话 blit
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
        ) -> PyResult<()> {
            let t = text
                .to_str()
                .ok_or_else(|| vm.new_value_error("文本含非 UTF-8 字符"))?;
            let surface = font
                .inner
                .render(t, true, [color.0[0], color.0[1], color.0[2], color.0[3]]);
            bridge::with_session(vm, |t| t.blit(&surface, (dest.0, dest.1)))
        }
    }

    // FillArgs 从 args 层引入（本 mod 内 pymethod 用）
    use args::FillArgs;

    // ── Color：颜色类型（批次三十九，替代旧 fn Color 函数——starfish
    //    pygame::color::Color 直通句柄；payload = Cell<Color>（Copy，
    //    内部可变性同款模式）。SfColor 有 downcast 分支（Color 实例直接作
    //    颜色参数））──
    #[pyattr]
    #[pyclass(name = "Color", unhashable = true)]
    #[derive(Debug, PyPayload)]
    pub struct PyColor(pub(crate) Cell<starfish_pygame::color::Color>);

    impl PyColor {
        /// 直通句柄读取（args::SfColor 直通分支用）
        pub(crate) fn get(&self) -> EngineColor {
            self.0.get()
        }
    }

    #[pyclass(with(Constructor, Comparable, Representable))]
    impl PyColor {
        #[pygetset]
        fn r(&self) -> u8 {
            self.0.get().r
        }
        #[pygetset(setter)]
        fn set_r(&self, v: u8) {
            let mut c = self.0.get();
            c.update(v, c.g, c.b, c.a);
            self.0.set(c);
        }
        #[pygetset]
        fn g(&self) -> u8 {
            self.0.get().g
        }
        #[pygetset(setter)]
        fn set_g(&self, v: u8) {
            let mut c = self.0.get();
            c.update(c.r, v, c.b, c.a);
            self.0.set(c);
        }
        #[pygetset]
        fn b(&self) -> u8 {
            self.0.get().b
        }
        #[pygetset(setter)]
        fn set_b(&self, v: u8) {
            let mut c = self.0.get();
            c.update(c.r, c.g, v, c.a);
            self.0.set(c);
        }
        #[pygetset]
        fn a(&self) -> u8 {
            self.0.get().a
        }
        #[pygetset(setter)]
        fn set_a(&self, v: u8) {
            let mut c = self.0.get();
            c.update(c.r, c.g, c.b, v);
            self.0.set(c);
        }

        /// 归一化 0.0..1.0 四元组（pygame 惯例）
        #[pymethod]
        fn normalize(&self) -> (f32, f32, f32, f32) {
            self.0.get().normalize()
        }

        /// 预乘 alpha（返回新 Color）
        #[pymethod]
        fn premul_alpha(&self, vm: &VirtualMachine) -> PyRef<Self> {
            Self(Cell::new(self.0.get().premul_alpha())).into_ref(&vm.ctx)
        }

        /// 线性插值（返回新 Color）
        #[pymethod]
        fn lerp(&self, other: PyRef<Self>, amount: f32, vm: &VirtualMachine) -> PyRef<Self> {
            Self(Cell::new(self.0.get().lerp(&other.0.get(), amount))).into_ref(&vm.ctx)
        }

        /// 灰度化（返回新 Color）
        #[pymethod]
        fn grayscale(&self, vm: &VirtualMachine) -> PyRef<Self> {
            Self(Cell::new(self.0.get().grayscale())).into_ref(&vm.ctx)
        }

        /// 反色（返回新 Color）
        #[pymethod]
        fn invert(&self, vm: &VirtualMachine) -> PyRef<Self> {
            Self(Cell::new(self.0.get().invert())).into_ref(&vm.ctx)
        }

        /// 伽马校正（返回新 Color）
        #[pymethod]
        fn correct_gamma(&self, gamma: f32, vm: &VirtualMachine) -> PyRef<Self> {
            Self(Cell::new(self.0.get().correct_gamma(gamma))).into_ref(&vm.ctx)
        }
    }

    impl Constructor for PyColor {
        type Args = FuncArgs;

        fn py_new(
            _cls: &Py<PyType>,
            args: Self::Args,
            vm: &VirtualMachine,
        ) -> PyResult<Self> {
            // 构造形态：Color() / Color(r,g,b) / Color(r,g,b,a) /
            // Color((r,g,b[,a])) / Color("name") / Color(Color)（拷贝）
            let c = match args.args.len() {
                0 => EngineColor::rgb(0, 0, 0),
                1 => {
                    let o = &args.args[0];
                    if let Some(s) = o.downcast_ref::<PyStr>() {
                        let name = s
                            .to_str()
                            .ok_or_else(|| vm.new_value_error("色彩名含非 UTF-8 字符"))?;
                        EngineColor::from_name(name).ok_or_else(|| {
                            vm.new_value_error(format!("未知色彩名：{name:?}"))
                        })?
                    } else if let Some(cc) = o.downcast_ref::<PyColor>() {
                        cc.0.get()
                    } else {
                        let SfColor(rgba) = SfColor::try_from_object(vm, o.clone())?;
                        EngineColor::new(rgba[0], rgba[1], rgba[2], rgba[3])
                    }
                }
                3 => {
                    let (r, g, b) = (
                        u8_of(&args.args[0], vm)?,
                        u8_of(&args.args[1], vm)?,
                        u8_of(&args.args[2], vm)?,
                    );
                    EngineColor::rgb(r, g, b)
                }
                4 => {
                    let (r, g, b, a) = (
                        u8_of(&args.args[0], vm)?,
                        u8_of(&args.args[1], vm)?,
                        u8_of(&args.args[2], vm)?,
                        u8_of(&args.args[3], vm)?,
                    );
                    EngineColor::new(r, g, b, a)
                }
                n => {
                    return Err(vm.new_type_error(format!(
                        "Color 构造须 0/1/3/4 参，实际 {n} 参"
                    )))
                }
            };
            Ok(Self(Cell::new(c)))
        }
    }

    impl Comparable for PyColor {
        fn cmp(
            zelf: &Py<Self>,
            other: &PyObject,
            op: PyComparisonOp,
            vm: &VirtualMachine,
        ) -> PyResult<PyComparisonValue> {
            if op != PyComparisonOp::Eq {
                return Ok(PyComparisonValue::NotImplemented);
            }
            let c = zelf.0.get();
            // Color == Color
            if let Some(o) = other.downcast_ref::<PyColor>() {
                return Ok(PyComparisonValue::Implemented(c == o.0.get()));
            }
            // Color == (r,g,b[,a])（兼容旧 fn Color 的元组契约）
            if let Some(t) = other.downcast_ref::<rustpython_vm::builtins::PyTuple>() {
                let items = t.as_slice();
                let eq = match items.len() {
                    3 => {
                        c.r as i32 == int_of(&items[0], vm)
                            && c.g as i32 == int_of(&items[1], vm)
                            && c.b as i32 == int_of(&items[2], vm)
                    }
                    4 => {
                        c.r as i32 == int_of(&items[0], vm)
                            && c.g as i32 == int_of(&items[1], vm)
                            && c.b as i32 == int_of(&items[2], vm)
                            && c.a as i32 == int_of(&items[3], vm)
                    }
                    _ => false,
                };
                return Ok(PyComparisonValue::Implemented(eq));
            }
            Ok(PyComparisonValue::NotImplemented)
        }
    }

    impl Representable for PyColor {
        fn repr(zelf: &Py<Self>, vm: &VirtualMachine) -> PyResult<PyRef<PyStr>> {
            let c = zelf.0.get();
            Ok(vm.ctx.new_str(format!("Color({}, {}, {}, {})", c.r, c.g, c.b, c.a)))
        }
    }

    /// 对象 → u8 通道值（越界报错）
    fn u8_of(o: &PyObjectRef, vm: &VirtualMachine) -> PyResult<u8> {
        let i = i32::try_from_object(vm, o.clone())
            .map_err(|_| vm.new_type_error("颜色分量须为整数"))?;
        if !(0..=255).contains(&i) {
            return Err(vm.new_type_error(format!("颜色分量越界（0..=255）：{i}")));
        }
        Ok(i as u8)
    }

    /// tuple 元素 → i32（Comparable 用；失败 = 0，仅比较语义）
    fn int_of(o: &PyObjectRef, vm: &VirtualMachine) -> i32 {
        i32::try_from_object(vm, o.clone()).unwrap_or(0)
    }
    /// 经典全量语义占位：引擎初始化由宿主壳完成，返回 (0, 0)。
    #[pyfunction]
    fn init() -> (usize, usize) {
        (0, 0)
    }

    /// 占位（引擎收口由宿主壳完成）。
    #[pyfunction]
    fn quit() {}
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
            starfish_pygame::render::session::begin_targets(&zelf.surfaces);
            zelf
        }

        /// 异常不吞（返回 False 交回宿主壳打印收口）。
        #[pymethod]
        fn __exit__(&self, _exc_info: FuncArgs) -> bool {
            starfish_pygame::render::session::end_targets();
            false
        }

        #[pymethod]
        fn fill(&self, args: args::FillArgs, vm: &VirtualMachine) -> PyResult<()> {
            let rgba = args.color.0;
            bridge::with_mrt_session(vm, |p| {
                use starfish_pygame::render::DrawTarget;
                p.fill(rgba);
            })
        }

        #[pymethod]
        fn rect(&self, args: args::DrawRectArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                starfish_pygame::draw::rect(
                    p,
                    args.color.0,
                    [args.rect.x, args.rect.y, args.rect.w, args.rect.h],
                    args.width.unwrap_or(0).max(0) as u32,
                );
            })
        }

        #[pymethod]
        fn circle(&self, args: args::DrawCircleArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                starfish_pygame::draw::circle(
                    p,
                    args.color.0,
                    (args.center.0, args.center.1),
                    args.radius.max(0) as u32,
                    args.width.unwrap_or(0).max(0) as u32,
                );
            })
        }

        #[pymethod]
        fn line(&self, args: args::DrawLineArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                starfish_pygame::draw::line(
                    p,
                    args.color.0,
                    (args.start.0, args.start.1),
                    (args.end.0, args.end.1),
                    args.width.unwrap_or(1).max(0) as u32,
                );
            })
        }

        #[pymethod]
        fn polygon(&self, args: args::DrawPolygonArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                starfish_pygame::draw::polygon(
                    p,
                    args.color.0,
                    &args.points.0,
                    args.width.unwrap_or(0).max(0) as u32,
                );
            })
        }

        #[pymethod]
        fn ellipse(&self, args: args::DrawEllipseArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                starfish_pygame::draw::ellipse(
                    p,
                    args.color.0,
                    [args.rect.x, args.rect.y, args.rect.w, args.rect.h],
                    args.width.unwrap_or(0).max(0) as u32,
                );
            })
        }

        #[pymethod]
        fn arc(&self, args: args::DrawArcArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                starfish_pygame::draw::arc(
                    p,
                    args.color.0,
                    [args.rect.x, args.rect.y, args.rect.w, args.rect.h],
                    args.start_angle.into_float() as f32,
                    args.stop_angle.into_float() as f32,
                    args.width.unwrap_or(1).max(0) as u32,
                );
            })
        }

        /// blit(src, dest)：Surface 贴入 MRT 会话。
        #[pymethod]
        fn blit(
            &self,
            src: PyRef<PySurface>,
            dest: args::SfPoint,
            vm: &VirtualMachine,
        ) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                use starfish_pygame::render::DrawTarget;
                p.blit(&src.surface, (dest.0, dest.1))
            })
        }

        /// blit_area(src, dest, area, color=None)：UV 子区域贴图
        /// （with_batch push_quad_uv 直通）。
        #[pymethod]
        fn blit_area(&self, args: args::BlitAreaArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| {
                bridge::blit_area(p, &args.src, args.dest, args.area, args.color, vm)
            })?
        }

        /// quad(texture, corners, uvs, color)：任意四角贴图（with_batch
        /// push_quad_corners 直通）。
        #[pymethod]
        fn quad(&self, args: args::QuadCornersArgs, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mrt_session(vm, |p| bridge::quad_corners(p, args, vm))?;
            Ok(())
        }
    }
}
