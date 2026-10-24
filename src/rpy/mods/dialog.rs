//! pygame.dialog 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod dialog {
        use rustpython_derive::pyclass;
        use rustpython_vm::convert::TryFromObject;
        use rustpython_vm::{PyObjectRef, PyPayload, PyResult, VirtualMachine, builtins::PyStrRef};

        use starfish_dialog::PickedFile;

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
                let picked = pollster::block_on(starfish_dialog::pick_file(
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
                let saved = pollster::block_on(starfish_dialog::save_bytes(name, bytes))
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

pub(crate) use dialog::module_def;
