//! pygame.gfxdraw 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod gfxdraw {
        use crate::rpy::pygame::pygame::{PyScreenSession, PySurfaceSession};
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
            let c = color.0;
            match route(&f, vm)? {
                Target::Surf(s) => {
                    starfish_pygame::draw::rect(&*s.surface, c,
                        [x, y, 1, 1], 0)
                }
                Target::Active => bridge::with_session(vm, |t| {
                    starfish_pygame::draw::rect(t, c,
                        [x, y, 1, 1], 0)
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
            let c = color.0;
            match route(&f, vm)? {
                Target::Surf(s) => starfish_pygame::draw::line(&*s.surface, c, (x1, y1), (x2, y2), 1),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish_pygame::draw::line(t, c, (x1, y1), (x2, y2), 1)
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
            let c = color.0;
            let r = radius.max(0) as u32;
            match route(&f, vm)? {
                Target::Surf(s) => starfish_pygame::draw::circle(&*s.surface, c, (x, y), r, 1),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish_pygame::draw::circle(t, c, (x, y), r, 1)
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
            let c = color.0;
            let r = radius.max(0) as u32;
            match route(&f, vm)? {
                Target::Surf(s) => starfish_pygame::draw::circle(&*s.surface, c, (x, y), r, 0),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish_pygame::draw::circle(t, c, (x, y), r, 0)
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
            let c = color.0;
            let rect = [x - rx, y - ry, rx.max(0) * 2, ry.max(0) * 2];
            match route(&f, vm)? {
                Target::Surf(s) => starfish_pygame::draw::ellipse(&*s.surface, c, rect, 1),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish_pygame::draw::ellipse(t, c, rect, 1)
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
            let c = color.0;
            let rect = [x - rx, y - ry, rx.max(0) * 2, ry.max(0) * 2];
            match route(&f, vm)? {
                Target::Surf(s) => starfish_pygame::draw::ellipse(&*s.surface, c, rect, 0),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish_pygame::draw::ellipse(t, c, rect, 0)
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
            let c = color.0;
            let rect = [x - radius, y - radius, radius.max(0) * 2, radius.max(0) * 2];
            let (s, e) = (
                (start_angle as f32).to_radians(),
                (stop_angle as f32).to_radians(),
            );
            match route(&f, vm)? {
                Target::Surf(sfc) => starfish_pygame::draw::arc(&*sfc.surface, c, rect, s, e, 1),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish_pygame::draw::arc(t, c, rect, s, e, 1)
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
            let c = color.0;
            let pts = SfPoints(vec![(x1, y1), (x2, y2), (x3, y3)]);
            match route(f, vm)? {
                Target::Surf(s) => starfish_pygame::draw::polygon(&*s.surface, c, &pts.0, 0),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish_pygame::draw::polygon(t, c, &pts.0, 0)
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
            let c = color.0;
            let pts = SfPoints(vec![(x1, y1), (x2, y2), (x3, y3)]);
            match route(f, vm)? {
                Target::Surf(s) => starfish_pygame::draw::polygon(&*s.surface, c, &pts.0, 1),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish_pygame::draw::polygon(t, c, &pts.0, 1)
                })?,
            };
            Ok(())
        }

        /// polygon(f, points, color)：1px 描边多边形（points = [(x, y), ...]）。
        #[pyfunction]
        fn polygon(f: PyObjectRef, points: SfPoints, color: SfColor, vm: &VirtualMachine) -> PyResult<()> {
            let c = color.0;
            match route(&f, vm)? {
                Target::Surf(s) => starfish_pygame::draw::polygon(&*s.surface, c, &points.0, 1),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish_pygame::draw::polygon(t, c, &points.0, 1)
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
            let c = color.0;
            match route(&f, vm)? {
                Target::Surf(s) => starfish_pygame::draw::polygon(&*s.surface, c, &points.0, 0),
                Target::Active => bridge::with_session(vm, |t| {
                    starfish_pygame::draw::polygon(t, c, &points.0, 0)
                })?,
            };
            Ok(())
        }

        /// rectangle(f, rect, color)：1px 描边矩形（rect = (left, top, w, h)）。
        #[pyfunction]
        fn rectangle(f: PyObjectRef, rect: SfRect, color: SfColor, vm: &VirtualMachine) -> PyResult<()> {
            let c = color.0;
            let r = [rect.x, rect.y, rect.w, rect.h];
            match route(&f, vm)? {
                Target::Surf(s) => starfish_pygame::draw::rect(&*s.surface, c, r, 1),
                Target::Active => bridge::with_session(vm, |t| starfish_pygame::draw::rect(t, c, r, 1))?,
            };
            Ok(())
        }

        /// box(f, rect, color)：填充矩形。
        #[pyfunction(name = "box")]
        fn box_(f: PyObjectRef, rect: SfRect, color: SfColor, vm: &VirtualMachine) -> PyResult<()> {
            let c = color.0;
            let r = [rect.x, rect.y, rect.w, rect.h];
            match route(&f, vm)? {
                Target::Surf(s) => starfish_pygame::draw::rect(&*s.surface, c, r, 0),
                Target::Active => bridge::with_session(vm, |t| starfish_pygame::draw::rect(t, c, r, 0))?,
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

pub(crate) use gfxdraw::module_def;
