//! pygame.io 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod io {
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
                let data = pollster::block_on(starfish_io::read(path_str(&path, vm)?))
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
                let data = pollster::block_on(starfish_io::read(path_str(&path, vm)?))
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
                pollster::block_on(starfish_io::write(path_str(&path, vm)?, bytes))
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
            pollster::block_on(starfish_io::exists(path_str(&path, vm)?))
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
                pollster::block_on(starfish_io::write_text(path_str(&path, vm)?, t))
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
                starfish_io::set_base_dir(path_str(&dir, vm)?);
                Ok(())
            }
        }
    }

    // ── net：网络（批次 M；轮询式语义——帧内 send/recv，主线程永不阻塞）──
    // TCP = 消息分帧连接（4 字节大端长度前缀，单帧上限 16 MB；原生后台
    // 线程 + std 阻塞流，Web 自动 WebSocket——三平台同一轮询面）。

pub(crate) use io::module_def;
