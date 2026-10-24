//! pygame.transform 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod transform {
        use rustpython_derive::pymodule;
        use rustpython_vm::function::ArgIntoFloat;
        use rustpython_vm::{PyPayload, PyRef, PyResult, VirtualMachine};
        use starfish_pygame::transform as sf_transform;

        use crate::rpy::pygame::pygame::PySurface;

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

pub(crate) use transform::module_def;
