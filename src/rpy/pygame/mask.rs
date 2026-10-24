//! pygame.mask 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod mask {
        use rustpython_derive::pyclass;
        use rustpython_vm::types::Constructor;
        use rustpython_vm::{
            Py, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine, builtins::PyType,
        };
        use starfish_pygame::mask::Mask as SfMask;

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
            fn to_surface(&self, vm: &VirtualMachine) -> PyRef<crate::rpy::pygame::pygame::PySurface> {
                let s = self.inner.borrow().to_surface();
                crate::rpy::pygame::pygame::PySurface { surface: std::sync::Arc::new(s) }.into_ref(&vm.ctx)
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
            surface: PyRef<crate::rpy::pygame::pygame::PySurface>,
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

pub(crate) use mask::module_def;
