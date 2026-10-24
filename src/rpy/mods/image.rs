//! pygame.image 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod image {
        use rustpython_derive::pymodule;
        use rustpython_vm::{PyPayload, PyRef, PyResult, VirtualMachine};
        use starfish_pygame::image as sf_image;

        use crate::rpy::pygame::pygame::PySurface;

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

pub(crate) use image::module_def;
