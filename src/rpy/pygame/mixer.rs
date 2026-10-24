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
        use rustpython_vm::{Py, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine, builtins::PyStrRef};
        use std::cell::{Cell, RefCell};
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

        /// set_reserved(n)：保留 0..n 通道（不参与 Sound.play 自动分配
        /// ——专供 Channel(idx) 定点使用）
        #[pyfunction]
        fn set_reserved(n: usize) {
            starfish_pygame::mixer::set_reserved(n)
        }

        /// num_channels() -> int：通道数（init 定格）
        #[pyfunction]
        fn num_channels() -> usize {
            starfish_pygame::mixer::num_channels()
        }

        /// output_sample_rate() -> int：混音输出采样率（Hz）
        #[pyfunction]
        fn output_sample_rate() -> u32 {
            starfish_pygame::mixer::output_sample_rate()
        }

        /// beep() -> Sound：内嵌提示音（CLOCK 权宜音——快速反馈用）
        #[pyfunction]
        fn beep(vm: &VirtualMachine) -> PyRef<PySound> {
            let data = starfish_pygame::mixer::beep();
            PySound {
                data,
                channel: std::cell::Cell::new(None),
            }
            .into_ref(&vm.ctx)
        }

        // ── Channel：定点通道（批次 49 parity；fade/特效挂载在此）──

        /// 定点通道（idx 不透明 int——0..num_channels；set_reserved 配合）
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.mixer", name = "Channel", unhashable = true)]
        pub(crate) struct PyChannel {
            pub(crate) inner: RefCell<starfish_pygame::mixer::Channel>,
        }

        impl std::fmt::Debug for PyChannel {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("Channel(<idx>)")
            }
        }

        /// Channel(idx)：定点通道句柄
        #[derive(FromArgs)]
        pub(crate) struct ChannelArgs {
            pub(crate) idx: usize,
        }

        impl Constructor for PyChannel {
            type Args = ChannelArgs;

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                _vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self {
                    inner: RefCell::new(starfish_pygame::mixer::Channel::new(args.idx)),
                })
            }
        }

        #[pyclass(with(Constructor))]
        impl PyChannel {
            /// play(sound, loops=0)：本通道定点播放
            #[pymethod]
            fn play(&self, sound: PyRef<PySound>, loops: Option<i32>, vm: &VirtualMachine) -> PyResult<()> {
                self.inner
                    .borrow()
                    .play(&starfish_pygame::mixer::Sound::from_data(sound.data.clone()), loops.unwrap_or(0))
                    .map_err(|e| vm.new_value_error(format!("音频错误: {e:?}")))
            }

            #[pymethod]
            fn stop(&self) {
                self.inner.borrow().stop()
            }

            #[pymethod]
            fn pause(&self) {
                self.inner.borrow().pause()
            }

            #[pymethod]
            fn resume(&self) {
                self.inner.borrow().resume()
            }

            /// set_volume(v)：本通道音量（0..1）
            #[pymethod]
            fn set_volume(&self, v: f64) {
                self.inner.borrow().set_volume(v as f32)
            }

            /// fade_in(sound, ms, loops=0)：本通道淡入播放
            #[pymethod]
            fn fade_in(
                &self,
                sound: PyRef<PySound>,
                ms: u32,
                loops: Option<i32>,
                vm: &VirtualMachine,
            ) -> PyResult<()> {
                self.inner
                    .borrow()
                    .fade_in(&starfish_pygame::mixer::Sound::from_data(sound.data.clone()), ms)
                    .map_err(|e| vm.new_value_error(format!("音频错误: {e:?}")))
            }

            /// fade_out(ms)：本通道淡出
            #[pymethod]
            fn fade_out(&self, ms: u32) {
                self.inner.borrow().fade_out(ms)
            }

            /// get_busy() -> bool：本通道播放中
            #[pymethod]
            fn get_busy(&self) -> bool {
                self.inner.borrow().get_busy()
            }

            /// get_sound() -> Sound | None：本通道当前装载的声音
            #[pymethod]
            fn get_sound(&self, vm: &VirtualMachine) -> PyResult<Option<PyRef<PySound>>> {
                Ok(self.inner.borrow().get_sound().map(|s| {
                    PySound {
                        data: s.data(),
                        channel: std::cell::Cell::new(Some(self.inner.borrow().idx())),
                    }
                    .into_ref(&vm.ctx)
                }))
            }

            /// add_effect(effect)：挂特效到本通道数据通路（Effect 句柄
            /// 克隆共享参数态——挂载后 `effect.set(key, v)` 仍实时生效）
            #[pymethod]
            fn add_effect(&self, effect: PyRef<PyEffect>) {
                self.inner.borrow().add_effect(effect.inner.borrow().clone())
            }
        }

        // ── Effect：音频特效（字符串键控参数面；Gain/Echo/Distortion/
        // LowPass 预设）──

        /// 音频特效句柄（kind = "gain" | "echo" | "distortion" | "lowpass"）
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.mixer", name = "Effect", unhashable = true)]
        pub(crate) struct PyEffect {
            pub(crate) inner: RefCell<starfish_pygame::effects::Effect>,
        }

        impl std::fmt::Debug for PyEffect {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("Effect(<preset>)")
            }
        }

        #[derive(FromArgs)]
        pub(crate) struct EffectArgs {
            #[pyarg(any)]
            pub(crate) kind: String,
        }

        impl Constructor for PyEffect {
            type Args = EffectArgs;

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                vm: &VirtualMachine,
            ) -> PyResult<Self> {
                let effect = starfish_pygame::effects::Effect::new(
                    &args.kind,
                    starfish_pygame::mixer::output_sample_rate(),
                )
                .map_err(|e| vm.new_value_error(format!("特效构造失败: {e:?}")))?;
                Ok(Self {
                    inner: RefCell::new(effect),
                })
            }
        }

        #[pyclass(with(Constructor))]
        impl PyEffect {
            /// set(key, value)：实时调参（键随预设：gain="gain"；
            /// echo="delay_ms"/"feedback"；distortion="drive"；
            /// lowpass="cutoff"）
            #[pymethod]
            fn set(&self, key: PyStrRef, value: f64, vm: &VirtualMachine) -> PyResult<()> {
                let k = key.to_str().ok_or_else(|| vm.new_type_error("键含非 UTF-8 字符"))?;
                self.inner
                    .borrow()
                    .set(k, value as f32)
                    .map_err(|e| vm.new_value_error(format!("{e:?}")))
            }
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


pub(crate) use mixer::{module_def, PySound, PyChannel, PyEffect};
