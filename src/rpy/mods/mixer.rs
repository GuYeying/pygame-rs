//! pygame.mixer 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod mixer {
        use rustpython_derive::{pymodule, pyclass, FromArgs};
        use rustpython_vm::types::Constructor;
        use rustpython_vm::builtins::PyType;
        use rustpython_vm::{Py, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine};
        use std::cell::Cell;
        use std::sync::Arc;
        use starfish_audio::SoundData;

        use crate::rpy::bridge;

        #[derive(Debug, PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.mixer", name = "Sound", unhashable = true)]
        pub(crate) struct PySound {
            pub(crate) data: Arc<SoundData>,
            /// 最近播放的通道（play 记录；set_volume/stop 作用于它——
            /// 通道索引停止后会被回收复用，长持需注意）
            pub(crate) channel: Cell<Option<usize>>,
        }

        /// Sound() 无参 = 内嵌提示音 / Sound("路径") / Sound(b"bytes")
        #[derive(FromArgs)]
        pub(crate) struct SoundArgs {
            #[pyarg(any, optional)]
            pub(crate) source: Option<PyObjectRef>,
        }

        impl Constructor for PySound {
            type Args = SoundArgs;

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self {
                    data: bridge::sound_source_to_data(args.source, vm)?,
                    channel: Cell::new(None),
                })
            }
        }

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

        /// pygame.mixer.init(channels=8)：显式初始化（不调也行——首次播放自动建）
        #[pyfunction]
        fn init(channels: Option<u32>, vm: &VirtualMachine) -> PyResult<()> {
            starfish_pygame::mixer::init(channels.unwrap_or(8))
                .map_err(|e| vm.new_value_error(format!("音频设备初始化失败: {e:?}")))
        }

        /// 卸载（Drop 即停设备）
        #[pyfunction]
        fn quit() {
            starfish_pygame::mixer::quit();
        }

        /// bool 形态（真 pygame 返回参数三元组/None——简化）
        #[pyfunction]
        fn get_init() -> bool {
            starfish_pygame::mixer::get_init()
        }

        /// 主音量（0..1）
        #[pyfunction]
        fn set_master_volume(v: f32, vm: &VirtualMachine) -> PyResult<()> {
            bridge::with_mixer(vm, |m| m.set_master_volume(v.clamp(0.0, 1.0)))
        }

        /// 任意 SFX/音乐播放中
        #[pyfunction]
        fn get_busy(vm: &VirtualMachine) -> PyResult<bool> {
            bridge::with_mixer(vm, |m| m.get_busy())
        }

        // ── pygame.mixer.music（流式 BGM；字节/文件双入口）──
        #[pymodule]
        mod music {
            use rustpython_derive::pymodule;
            use rustpython_vm::{PyResult, VirtualMachine};

            use crate::rpy::bridge;

            #[pyfunction]
            fn load(path: String, vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_load_file(&path))?;
                Ok(())
            }

            /// 字节流式加载（内嵌 BGM 零落盘；starfish music_load_bytes）
            #[pyfunction]
            fn load_bytes(bytes: Vec<u8>, vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_load_bytes(bytes))?;
                Ok(())
            }

            /// play(loops=0)：-1 无限 / 0 单次 / N 次
            #[derive(rustpython_derive::FromArgs)]
            pub(crate) struct MusicPlayArgs {
                #[pyarg(any, optional)]
                pub(crate) loops: Option<i32>,
            }

            #[pyfunction]
            fn play(args: MusicPlayArgs, vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_play(args.loops.unwrap_or(0)))?;
                Ok(())
            }

            #[pyfunction]
            fn stop(vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_stop())?;
                Ok(())
            }

            #[pyfunction]
            fn pause(vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_pause())?;
                Ok(())
            }

            #[pyfunction]
            fn resume(vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_resume())?;
                Ok(())
            }

            /// 音乐音量（0..1）
            #[pyfunction]
            fn set_volume(v: f32, vm: &VirtualMachine) -> PyResult<()> {
                bridge::with_mixer(vm, |m| m.music_set_volume(v.clamp(0.0, 1.0)))
            }

            /// 音乐播放中（不含 SFX——与 mixer.get_busy 分工）
            #[pyfunction]
            fn get_busy(vm: &VirtualMachine) -> PyResult<bool> {
                bridge::with_mixer(vm, |m| m.music_is_playing())
            }
        }
    }


pub(crate) use mixer::{module_def, PySound};
