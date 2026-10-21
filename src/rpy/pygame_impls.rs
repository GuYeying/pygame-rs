//! pygame pyclass 方法实现层（批次 D 拆分实验）。
//!
//! struct 定义 + pyattr/pyclass 注册留在 `native_pygame::pygame`（pymodule
//! 宏要求树内声明）；本文件只放 `#[pyclass] impl` 方法收集块 + 协议 impl
//! （impl 块与 struct 分离——同 crate 内 `impl 本地类型` 合法，pyclass 宏
//! 按类型关联，与所在模块无关）。

use std::cell::Cell;
use rustpython_derive::{pyclass, FromArgs};
use rustpython_vm::function::{FuncArgs, PyComparisonValue};
use rustpython_vm::types::{Comparable, Constructor, PyComparisonOp, Representable};
use rustpython_vm::convert::TryFromObject;
use rustpython_vm::{
    Py, PyObject, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine,
    builtins::{PyStr, PyType},
};
use starfish_pygame::color::Color as EngineColor;
use starfish_pygame::Rect as EngineRect;
use starfish_pygame::render::{DrawTarget, Surface as SfSurface};

use crate::rpy::native_pygame::pygame::font::PyFont;
use crate::rpy::native_pygame::pygame::DEFAULT_FONT;
use crate::rpy::bridge;
use crate::rpy::native_pygame::pygame::mixer::PySound;
use crate::rpy::native_pygame::pygame::PySurface;
use crate::rpy::native_pygame::pygame::{PyColor, PyRect, PyScreenSession, PySurfaceSession};
use crate::rpy::args::{SfColor, SfPoint, SfRect};
use crate::rpy::native_pygame::pygame::event::PyKey;

/// Session 绘制方法的 Rect 形参别名
type PyRectRef = PyRef<PyRect>;

    impl PyRect {
        /// 直通句柄读取（args::SfRect 直通分支用）
        pub(crate) fn get(&self) -> EngineRect {
            self.0.get()
        }
    }

    #[pyclass(with(Constructor, Comparable, Representable))]
    impl PyRect {
        #[pymethod]
        fn copy(&self, vm: &VirtualMachine) -> PyResult<PyRef<PyRect>> {
            Ok(PyRect(Cell::new(self.0.get())).into_ref(&vm.ctx))
        }

        // ── 读写属性（left/top/width/height + x/y/w/h 别名 +
        //    right/bottom/centerx/centery 可写计算属性）──
        #[pygetset]
        fn left(&self) -> i32 {
            self.0.get().left
        }
        #[pygetset(setter)]
        fn set_left(&self, v: i32) {
            let mut r = self.0.get();
            r.left = v;
            self.0.set(r);
        }
        #[pygetset]
        fn top(&self) -> i32 {
            self.0.get().top
        }
        #[pygetset(setter)]
        fn set_top(&self, v: i32) {
            let mut r = self.0.get();
            r.top = v;
            self.0.set(r);
        }
        #[pygetset]
        fn width(&self) -> i32 {
            self.0.get().width
        }
        #[pygetset(setter)]
        fn set_width(&self, v: i32) {
            let mut r = self.0.get();
            r.width = v;
            self.0.set(r);
        }
        #[pygetset]
        fn height(&self) -> i32 {
            self.0.get().height
        }
        #[pygetset(setter)]
        fn set_height(&self, v: i32) {
            let mut r = self.0.get();
            r.height = v;
            self.0.set(r);
        }
        // pygame 惯例别名
        #[pygetset(name = "x")]
        fn x(&self) -> i32 {
            self.left()
        }
        #[pygetset(name = "x", setter)]
        fn set_x(&self, v: i32) {
            self.set_left(v);
        }
        #[pygetset(name = "y")]
        fn y(&self) -> i32 {
            self.top()
        }
        #[pygetset(name = "y", setter)]
        fn set_y(&self, v: i32) {
            self.set_top(v);
        }
        #[pygetset(name = "w")]
        fn w(&self) -> i32 {
            self.width()
        }
        #[pygetset(name = "w", setter)]
        fn set_w(&self, v: i32) {
            self.set_width(v);
        }
        #[pygetset(name = "h")]
        fn h(&self) -> i32 {
            self.height()
        }
        #[pygetset(name = "h", setter)]
        fn set_h(&self, v: i32) {
            self.set_height(v);
        }
        // 可写计算属性（纯算术：right = left + width 等）
        #[pygetset]
        fn right(&self) -> i32 {
            self.0.get().right()
        }
        #[pygetset(setter)]
        fn set_right(&self, v: i32) {
            let mut r = self.0.get();
            r.left = v - r.width;
            self.0.set(r);
        }
        #[pygetset]
        fn bottom(&self) -> i32 {
            self.0.get().bottom()
        }
        #[pygetset(setter)]
        fn set_bottom(&self, v: i32) {
            let mut r = self.0.get();
            r.top = v - r.height;
            self.0.set(r);
        }
        #[pygetset]
        fn centerx(&self) -> i32 {
            self.0.get().left + self.0.get().width / 2
        }
        #[pygetset(setter)]
        fn set_centerx(&self, v: i32) {
            let mut r = self.0.get();
            r.left = v - r.width / 2;
            self.0.set(r);
        }
        #[pygetset]
        fn centery(&self) -> i32 {
            self.0.get().top + self.0.get().height / 2
        }
        #[pygetset(setter)]
        fn set_centery(&self, v: i32) {
            let mut r = self.0.get();
            r.top = v - r.height / 2;
            self.0.set(r);
        }
        // 只读 tuple 视图（v1）
        #[pygetset]
        fn size(&self, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let r = self.0.get();
            Ok(vm.new_tuple((r.width, r.height)).into())
        }
        #[pygetset]
        fn topleft(&self, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let r = self.0.get();
            Ok(vm.new_tuple((r.left, r.top)).into())
        }
        #[pygetset]
        fn center(&self, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let (x, y) = self.0.get().center();
            Ok(vm.new_tuple((x, y)).into())
        }

        // ── 方法（真身 = starfish Rect 同名方法；签名差异见注释）──
        #[pymethod(name = "move")]
        fn move_(&self, dx: i32, dy: i32, vm: &VirtualMachine) -> PyResult<PyRef<PyRect>> {
            Ok(PyRect(Cell::new(self.0.get().move_(dx, dy))).into_ref(&vm.ctx))
        }
        #[pymethod]
        fn move_ip(&self, dx: i32, dy: i32) {
            self.0.set(self.0.get().move_(dx, dy));
        }
        #[pymethod]
        fn inflate(&self, dw: i32, dh: i32, vm: &VirtualMachine) -> PyResult<PyRef<PyRect>> {
            Ok(PyRect(Cell::new(self.0.get().inflate(dw, dh))).into_ref(&vm.ctx))
        }
        #[pymethod]
        fn inflate_ip(&self, dw: i32, dh: i32) {
            self.0.set(self.0.get().inflate(dw, dh));
        }
        #[pymethod]
        fn scale_by(&self, mul: f32, vm: &VirtualMachine) -> PyResult<PyRef<PyRect>> {
            Ok(PyRect(Cell::new(self.0.get().scale_by(mul))).into_ref(&vm.ctx))
        }
        #[pymethod]
        fn scale_by_ip(&self, mul: f32) {
            self.0.set(self.0.get().scale_by(mul));
        }
        /// update(left, top, width, height)：四参原地更新（pygame 的
        /// tuple/(x,y)+size 形态留 v2）
        #[pymethod]
        fn update(&self, left: i32, top: i32, width: i32, height: i32) {
            self.0.set(EngineRect::new(left, top, width, height));
        }
        #[pymethod]
        fn clamp(&self, other: PyRef<PyRect>, vm: &VirtualMachine) -> PyResult<PyRef<PyRect>> {
            Ok(PyRect(Cell::new(self.0.get().clamp(other.get()))).into_ref(&vm.ctx))
        }
        #[pymethod]
        fn clamp_ip(&self, other: PyRef<PyRect>) {
            self.0.set(self.0.get().clamp(other.get()));
        }
        #[pymethod]
        fn clip(&self, other: PyRef<PyRect>, vm: &VirtualMachine) -> PyResult<PyRef<PyRect>> {
            Ok(PyRect(Cell::new(self.0.get().clip(other.get()))).into_ref(&vm.ctx))
        }
        #[pymethod]
        fn contains(&self, other: PyRef<PyRect>) -> bool {
            self.0.get().contains(other.get())
        }
        #[pymethod]
        fn collidepoint(&self, p: SfPoint) -> bool {
            self.0.get().collidepoint((p.0, p.1))
        }
        #[pymethod]
        fn colliderect(&self, other: PyRef<PyRect>) -> bool {
            self.0.get().colliderect(&other.get())
        }
        #[pymethod]
        fn collidelist(&self, rects: PyObjectRef, vm: &VirtualMachine) -> PyResult<i32> {
            let list = rect_list(&rects, vm)?;
            Ok(self.0.get().collidelist(&list))
        }
        #[pymethod]
        fn collidelistall(&self, rects: PyObjectRef, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let list = rect_list(&rects, vm)?;
            Ok(vm
                .ctx
                .new_list(self.0.get().collidelistall(&list).into_iter().map(|i| vm.ctx.new_int(i).into()).collect())
                .into())
        }
        #[pymethod]
        fn union(&self, other: PyRef<PyRect>, vm: &VirtualMachine) -> PyResult<PyRef<PyRect>> {
            Ok(PyRect(Cell::new(self.0.get().union(other.get()))).into_ref(&vm.ctx))
        }
        #[pymethod]
        fn union_ip(&self, other: PyRef<PyRect>) {
            self.0.set(self.0.get().union(other.get()));
        }
        #[pymethod]
        fn fit(&self, target: PyRef<PyRect>, vm: &VirtualMachine) -> PyResult<PyRef<PyRect>> {
            Ok(PyRect(Cell::new(self.0.get().fit(target.get()))).into_ref(&vm.ctx))
        }
        /// normalize：**与 pygame 有差异**——starfish 返回新 Rect（非原地），
        /// 随其接口形态；原地效果用 `r.update(r.normalize()...)` 或等 v2
        #[pymethod]
        fn normalize(&self, vm: &VirtualMachine) -> PyResult<PyRef<PyRect>> {
            Ok(PyRect(Cell::new(self.0.get().normalize())).into_ref(&vm.ctx))
        }
    }

    impl Constructor for PyRect {
        type Args = (i32, i32, i32, i32);

        fn py_new(
            _cls: &Py<PyType>,
            args: Self::Args,
            _vm: &VirtualMachine,
        ) -> PyResult<Self> {
            Ok(Self(Cell::new(EngineRect::new(args.0, args.1, args.2, args.3))))
        }
    }

    impl Comparable for PyRect {
        fn cmp(
            zelf: &Py<Self>,
            other: &PyObject,
            op: PyComparisonOp,
            _vm: &VirtualMachine,
        ) -> PyResult<PyComparisonValue> {
            if op == PyComparisonOp::Eq {
                if let Some(o) = other.downcast_ref::<PyRect>() {
                    return Ok(PyComparisonValue::Implemented(zelf.0.get() == o.0.get()));
                }
            }
            Ok(PyComparisonValue::NotImplemented)
        }
    }

    impl Representable for PyRect {
        fn repr(zelf: &Py<Self>, vm: &VirtualMachine) -> PyResult<PyRef<PyStr>> {
            let r = zelf.0.get();
            Ok(vm.ctx.new_str(format!(
                "<Rect({}, {}, {}, {})>",
                r.left, r.top, r.width, r.height
            )))
        }
    }

    /// Rect 实例序列 → Vec<EngineRect>（collidelist* 的参数整形）。
    fn rect_list(obj: &PyObjectRef, vm: &VirtualMachine) -> PyResult<Vec<EngineRect>> {
        vm.extract_elements_with(obj, |o| {
            PyRectRef::try_from_object(vm, o).map(|r| r.get())
        })
    }

    // ── Color（批次三十九：替代旧 fn Color 函数——starfish Color 直通）──
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

#[pyclass(with(Comparable))]
impl PyKey {}

impl Comparable for PyKey {
        fn cmp(
            zelf: &Py<Self>,
            other: &PyObject,
            op: PyComparisonOp,
            _vm: &VirtualMachine,
        ) -> PyResult<PyComparisonValue> {
            if op == PyComparisonOp::Eq {
                if let Some(o) = other.downcast_ref::<PyKey>() {
                    return Ok(PyComparisonValue::Implemented(zelf.0 == o.0));
                }
            }
            Ok(PyComparisonValue::NotImplemented)
        }
    }

// ── PySound（批次 E）：SFX 播放控制 ──────────────────────────────

/// play(loops=0) 收参（-1 无限 / 0 单次 / N 次）
#[derive(FromArgs)]
pub(crate) struct PlayArgs {
    #[pyarg(any, optional)]
    pub(crate) loops: Option<i32>,
}

#[pyclass(with(Constructor))]
impl PySound {
    /// play(loops=0)：-1 无限 / 0 单次 / N 次；记录通道供 stop/音量定位
    /// （通道索引停止后会被回收复用——starfish 通道模型固有限制）。
    #[pymethod]
    fn play(&self, args: PlayArgs, vm: &VirtualMachine) -> PyResult<()> {
        let ch = bridge::with_mixer(vm, |m| {
            m.play_with(self.data.clone(), args.loops.unwrap_or(0))
                .map_err(|e| vm.new_value_error(format!("音频错误: {e:?}")))
        })??;
        self.channel.set(ch);
        Ok(())
    }

    #[pymethod]
    fn stop(&self, vm: &VirtualMachine) -> PyResult<()> {
        if let Some(ch) = self.channel.get() {
            bridge::with_mixer(vm, |m| m.stop(ch))?;
        }
        Ok(())
    }

    /// set_volume(v)：作用于最近通道（0..1）
    #[pymethod]
    fn set_volume(&self, v: f32, vm: &VirtualMachine) -> PyResult<()> {
        if let Some(ch) = self.channel.get() {
            bridge::with_mixer(vm, |m| m.set_channel_volume(ch, v.clamp(0.0, 1.0)))?;
        }
        Ok(())
    }

    /// get_length() -> f32：时长（秒）
    #[pymethod]
    fn get_length(&self) -> f32 {
        self.data.duration()
    }
}

// ── PySurface（批次 F）：starfish Surface 直通句柄 ────────────────

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
    fn render(&self, vm: &VirtualMachine) -> PyResult<PyRef<crate::rpy::native_pygame::pygame::PySurfaceSession>> {
        Ok(crate::rpy::native_pygame::pygame::PySurfaceSession {
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
            starfish_pygame::Rect::new(r.x, r.y, r.w, r.h)
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

    /// fill(color) -> Rect：整面填充（DrawTarget 直通）
    #[pymethod]
    fn fill(&self, color: SfColor, vm: &VirtualMachine) -> PyResult<PyRef<PyRect>> {
        let r = DrawTarget::fill(&*self.surface, [color.0[0], color.0[1], color.0[2], color.0[3]]);
        Ok(PyRect(Cell::new(r)).into_ref(&vm.ctx))
    }

    /// blit(src, dest) -> Rect：Surface 贴图（self-blit 会 panic——
    /// starfish assert_not_self，文档标注）
    #[pymethod]
    fn blit(
        &self,
        src: PyRef<PySurface>,
        dest: SfPoint,
        vm: &VirtualMachine,
    ) -> PyResult<PyRef<PyRect>> {
        let r = DrawTarget::blit(&*self.surface, &src.surface, (dest.0, dest.1));
        Ok(PyRect(Cell::new(r)).into_ref(&vm.ctx))
    }

    /// get_rect() -> Rect(0, 0, w, h)：尺寸矩形（批次 C 复用）
    #[pymethod]
    fn get_rect(&self, vm: &VirtualMachine) -> PyResult<PyRef<PyRect>> {
        let (w, h) = self.surface.size();
        Ok(PyRect(Cell::new(EngineRect::new(0, 0, w as i32, h as i32))).into_ref(&vm.ctx))
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

// ── 会话相机/裁剪（批次 B：r.set_clip / r.set_camera_*）────────────
// Screen 会话相机走统一 RenderPass::set_camera（MRT rig 亦同臂）；
// Surface 离屏会话 v1 只补 set_clip（相机随 3D 内容 v2 开放）。
// ⚠️ RP 同类型仅允许一个 #[pyclass] impl 块——Session/SurfaceSession
// 的方法并入 native_pygame 的既有块（本文件只承载"块不在 native"的
// 类型：PyRect/PySurface/PyKey...）。
