//! pygame.video 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod video {
        use rustpython_derive::pyclass;
        use rustpython_vm::types::Constructor;
        use rustpython_vm::{
            Py, PyPayload, PyRef, PyResult, VirtualMachine,
            builtins::{PyStrRef, PyType},
        };
        use starfish_pygame::video::{VideoSystem as SfVideoSystem, Video as SfVideo};

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

pub(crate) use video::module_def;
