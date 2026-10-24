//! pygame.font 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod font {
        use rustpython_derive::pyclass;
        use rustpython_vm::types::Constructor;
        use starfish_pygame::font as sf_font;

        use crate::rpy::bridge;
        use rustpython_vm::builtins::{PyModule, PyStrRef, PyType};
        use rustpython_vm::{Py, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine};

        // 默认字体 = 顶层常量（见 mod pygame 顶部 DEFAULT_FONT 定义——维护者
        // 自定义入口，本模块不重复声明）
        use crate::rpy::pygame::pygame::DEFAULT_FONT;

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
            ) -> PyResult<rustpython_vm::PyRef<super::PySurface>> {
                let t = text
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("文本含非 UTF-8 字符"))?;
                let surface = self.inner.render(
                    t,
                    antialias.unwrap_or(true),
                    [color.0[0], color.0[1], color.0[2], color.0[3]],
                );
                Ok(crate::rpy::pygame::pygame::PySurface { surface: std::sync::Arc::new(surface) }.into_ref(&vm.ctx))
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
                    if let Some(bytes) = sf_font::preloaded(name) {
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

pub(crate) use font::module_def;

pub(crate) use font::PyFont;
