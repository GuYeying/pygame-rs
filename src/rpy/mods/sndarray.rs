//! pygame.sndarray 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    pub mod sndarray {
        use rustpython_derive::pymodule;
        use rustpython_vm::convert::TryFromObject;
        use rustpython_vm::{PyObjectRef, PyRef, PyResult, VirtualMachine};

        use crate::rpy::mods::mixer::PySound;

        /// array(sound) -> [[L, R], ...]（立体声）/ [[v], ...]（单声道）——
        /// 无 numpy 环境用嵌套 list 承载（真 pygame 是 numpy 数组，差异标注）
        #[pyfunction]
        fn array(sound: PyRef<PySound>, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
            let data = sound.data.clone();
            let rows: Vec<PyObjectRef> = match &data.channels {
                starfish_audio::AudioChannels::Mono(samples) => samples
                    .iter()
                    .map(|v| vm.new_tuple((*v,)).into())
                    .collect(),
                starfish_audio::AudioChannels::Stereo(frames) => frames
                    .iter()
                    .map(|f| vm.new_tuple((f.left, f.right)).into())
                    .collect(),
            };
            Ok(vm.ctx.new_list(rows).into())
        }

        /// make_sound(嵌套list) -> Sound：行宽 1 = 单声道 / 2 = 立体声（可混排
        /// 以先出现的行宽为准；采样率 v1 固定 44100）
        #[pyfunction]
        fn make_sound(array: PyObjectRef, vm: &VirtualMachine) -> PyResult<PySound> {
            use rustpython_vm::convert::TryFromObject;
            let rows = vm.extract_elements_with(&array, |o| {
                vm.extract_elements_with(&o, |v| f32::try_from_object(vm, v))
            })?;
            let mut flat: Vec<f32> = Vec::new();
            let mut channels = 0usize;
            for row in &rows {
                match row.len() {
                    1 => {
                        if channels == 0 {
                            channels = 1;
                        }
                        flat.push(row[0]);
                    }
                    2 => {
                        if channels == 0 {
                            channels = 2;
                        }
                        if channels != 2 {
                            return Err(vm.new_value_error("行宽不一致（单声道与立体声混排）"));
                        }
                        flat.push(row[0]);
                        flat.push(row[1]);
                    }
                    _ => {
                        return Err(vm.new_value_error(format!(
                            "行宽须为 1（单声道）或 2（立体声），得到 {}",
                            row.len()
                        )))
                    }
                }
            }
            let rate = 44_100u32; // v1 固定（真 pygame 从 array 推断采样率的形态留 v2）
            let data = match channels {
                1 => starfish_audio::SoundData::from_mono_f32(&flat, rate),
                _ => starfish_audio::SoundData::from_interleaved_f32(&flat, rate),
            };
            Ok(PySound {
                data: std::sync::Arc::new(data),
                channel: std::cell::Cell::new(None),
            })
        }
    }

pub(crate) use sndarray::module_def;
