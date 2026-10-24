//! pygame.sprite 绑定（2026-10-24 自 native_pygame.rs 拆分；body 原样搬运）。
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
    mod sprite {
        use rustpython_derive::pyclass;
        use rustpython_vm::types::Constructor;
        use rustpython_vm::{
            Py, PyObjectRef, PyPayload, PyRef, PyResult, VirtualMachine,
            builtins::{PyStrRef, PyType},
        };
        use starfish_pygame::sprite as sf_sprite;

        use crate::rpy::args::SfPoint;

        /// 组条目的完整规格（add 时一次性物化）
        struct SpriteSpec {
            at: (i32, i32),
            tag: Option<String>,
            z: i32,
            rotation: f32,
            scale: f32,
            alpha: Option<f32>,
            visible: bool,
            fps: f32,
        }

        /// pygame.sprite.Sprite——配置对象；Group.add 后成为组句柄。
        /// 单帧 = Surface；多帧 = [Surface, ...]（帧动画，fps 由
        /// `set_fps` 定，缺省 4）。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.sprite", name = "Sprite", unhashable = true)]
        pub(crate) struct PySprite {
            pub(crate) surfaces: Vec<std::sync::Arc<starfish_pygame::render::Surface>>,
            pub(crate) spec: std::cell::RefCell<SpriteSpec>,
            pub(crate) id: std::cell::Cell<Option<sf_sprite::SpriteId>>,
        }

        impl std::fmt::Debug for PySprite {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&format!("Sprite({} 帧, x{} 帧)", self.surfaces.len(), self.spec.borrow().at.0))
            }
        }

        impl Constructor for PySprite {
            type Args = (PyObjectRef,);

            fn py_new(
                _cls: &Py<PyType>,
                args: Self::Args,
                vm: &VirtualMachine,
            ) -> PyResult<Self> {
                let (img,) = args;
                // Surface 实例或 [Surface, ...] 列表双形态
                let surfaces: Vec<std::sync::Arc<starfish_pygame::render::Surface>> = if let Some(s) =
                    img.downcast_ref::<crate::rpy::pygame::pygame::PySurface>()
                {
                    vec![s.surface.clone()]
                } else {
                    vm.extract_elements_with(&img, |o: PyObjectRef| {
                        o.downcast_ref::<crate::rpy::pygame::pygame::PySurface>()
                            .map(|s| s.surface.clone())
                            .ok_or_else(|| {
                                vm.new_type_error("Sprite 参数须为 Surface 或 Surface 列表")
                            })
                    })?
                };
                if surfaces.is_empty() {
                    return Err(vm.new_value_error("Sprite 至少需要一帧 Surface"));
                }
                Ok(Self {
                    surfaces,
                    spec: std::cell::RefCell::new(SpriteSpec {
                        at: (0, 0),
                        tag: None,
                        z: 0,
                        rotation: 0.0,
                        scale: 1.0,
                        alpha: None,
                        visible: true,
                        fps: 4.0,
                    }),
                    id: std::cell::Cell::new(None),
                })
            }
        }

        #[pyclass(with(Constructor))]
        impl PySprite {
            /// at(x, y) -> self：初始位置（链式）
            #[pymethod]
            fn at(zelf: PyRef<Self>, x: i32, y: i32) -> PyRef<Self> {
                zelf.spec.borrow_mut().at = (x, y);
                zelf
            }

            /// tag(name) -> self：分组标签（链式；with_tag 查询用）
            #[pymethod]
            fn tag(zelf: PyRef<Self>, tag: PyStrRef, vm: &VirtualMachine) -> PyResult<PyRef<Self>> {
                let t = tag
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("tag 含非 UTF-8 字符"))?
                    .to_owned();
                zelf.spec.borrow_mut().tag = Some(t);
                Ok(zelf)
            }

            /// z(v) -> self：层级（(z, y) 排序会话绘制）
            #[pymethod]
            fn z(zelf: PyRef<Self>, v: i32) -> PyRef<Self> {
                zelf.spec.borrow_mut().z = v;
                zelf
            }

            /// rotation(deg) -> self：旋转角（度）
            #[pymethod]
            fn rotation(zelf: PyRef<Self>, deg: f32) -> PyRef<Self> {
                zelf.spec.borrow_mut().rotation = deg;
                zelf
            }

            /// scale(f) -> self：等比缩放
            #[pymethod]
            fn scale(zelf: PyRef<Self>, f: f32) -> PyRef<Self> {
                zelf.spec.borrow_mut().scale = f;
                zelf
            }

            /// alpha(a) -> self：表面级半透明（0.0..1.0）
            #[pymethod]
            fn alpha(zelf: PyRef<Self>, a: f32) -> PyRef<Self> {
                zelf.spec.borrow_mut().alpha = Some(a.clamp(0.0, 1.0));
                zelf
            }

            /// visible(b) -> self：可见性
            #[pymethod]
            fn visible(zelf: PyRef<Self>, b: bool) -> PyRef<Self> {
                zelf.spec.borrow_mut().visible = b;
                zelf
            }

            /// set_fps(f)：多帧动画帧率（缺省 4）
            #[pymethod]
            fn set_fps(&self, f: f32) {
                self.spec.borrow_mut().fps = f.max(0.1);
            }

            /// alive() -> bool：是否已在组内存活（未 add = False）
            #[pymethod]
            fn alive(&self) -> bool {
                self.id.get().is_some()
            }
        }

        /// pygame.sprite.Group——池化组（世代 id；remove 后旧句柄自动失效）。
        #[derive(PyPayload)]
        #[pyattr]
        #[pyclass(module = "pygame.sprite", name = "Group", unhashable = true)]
        pub(crate) struct PyGroup {
            pub(crate) inner: std::cell::RefCell<sf_sprite::Group>,
        }

        impl std::fmt::Debug for PyGroup {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("Group(<pooled>)")
            }
        }

        fn spec_to_sprite(sprite: &PySprite, vm: &VirtualMachine) -> PyResult<sf_sprite::Sprite> {
            let spec = sprite.spec.borrow();
            let mut it = if sprite.surfaces.len() >= 2 {
                let refs: Vec<&starfish_pygame::render::Surface> =
                    sprite.surfaces.iter().map(|s| s.as_ref()).collect();
                sf_sprite::Sprite::with_frames(&refs, spec.fps)
            } else {
                sf_sprite::Sprite::new(&sprite.surfaces[0])
            };
            it = it.at(spec.at.0, spec.at.1).z(spec.z).scale(spec.scale).visible(spec.visible);
            if spec.rotation != 0.0 {
                it = it.rotated(spec.rotation);
            }
            if let Some(a) = spec.alpha {
                it = it.alpha(a);
            }
            if let Some(t) = &spec.tag {
                let leaked: &'static str = t.clone().leak();
                it = it.tag(leaked);
            }
            let _ = vm;
            Ok(it)
        }

        fn id_of(sprite: &PySprite, vm: &VirtualMachine) -> PyResult<sf_sprite::SpriteId> {
            sprite.id.get().ok_or_else(|| {
                vm.new_value_error("Sprite 尚未加入任何 Group（先 group.add(sprite)）")
            })
        }

        #[pyclass(with(Constructor))]
        impl PyGroup {

            /// add(sprite) -> sprite：物化配置入组并回写世代 id
            /// （返回原对象以便链式/即取即用）。
            #[pymethod]
            fn add(&self, sprite: PyRef<PySprite>, vm: &VirtualMachine) -> PyResult<PyRef<PySprite>> {
                let it = spec_to_sprite(&sprite, vm)?;
                let id = self.inner.borrow_mut().insert(it);
                sprite.id.set(Some(id));
                Ok(sprite)
            }

            /// remove(sprite) -> bool：出组（世代 id 失效，旧句柄安全）。
            #[pymethod]
            fn remove(&self, sprite: PyRef<PySprite>, vm: &VirtualMachine) -> PyResult<bool> {
                let id = id_of(&sprite, vm)?;
                let removed = self.inner.borrow_mut().remove(id);
                if removed {
                    sprite.id.set(None);
                }
                Ok(removed)
            }

            /// alive(sprite) -> bool：句柄是否存活。
            #[pymethod]
            fn alive(&self, sprite: PyRef<PySprite>, vm: &VirtualMachine) -> PyResult<bool> {
                match sprite.id.get() {
                    Some(id) => Ok(self.inner.borrow().is_alive(id)),
                    None => Ok(false),
                }
            }

            /// count() -> int：组内存活精灵数。
            #[pymethod]
            fn count(&self) -> usize {
                self.inner.borrow().count()
            }

            /// position(sprite) -> (x, y) | None。
            #[pymethod]
            fn position(
                &self,
                sprite: PyRef<PySprite>,
                vm: &VirtualMachine,
            ) -> PyResult<Option<(i32, i32)>> {
                let id = id_of(&sprite, vm)?;
                Ok(self.inner.borrow().position(id))
            }

            /// move_by(sprite, dx, dy)：位移。
            #[pymethod]
            fn move_by(&self, sprite: PyRef<PySprite>, dx: i32, dy: i32, vm: &VirtualMachine) -> PyResult<()> {
                let id = id_of(&sprite, vm)?;
                self.inner.borrow_mut().move_by(id, dx, dy);
                Ok(())
            }

            /// set_position(sprite, x, y)。
            #[pymethod]
            fn set_position(&self, sprite: PyRef<PySprite>, x: i32, y: i32, vm: &VirtualMachine) -> PyResult<()> {
                let id = id_of(&sprite, vm)?;
                self.inner.borrow_mut().set_position(id, x, y);
                Ok(())
            }

            /// set_z(sprite, v)：层级。
            #[pymethod]
            fn set_z(&self, sprite: PyRef<PySprite>, v: i32, vm: &VirtualMachine) -> PyResult<()> {
                let id = id_of(&sprite, vm)?;
                self.inner.borrow_mut().set_z(id, v);
                Ok(())
            }

            /// set_rotation(sprite, deg)。
            #[pymethod]
            fn set_rotation(&self, sprite: PyRef<PySprite>, deg: f32, vm: &VirtualMachine) -> PyResult<()> {
                let id = id_of(&sprite, vm)?;
                self.inner.borrow_mut().set_rotation(id, deg);
                Ok(())
            }

            /// set_scale(sprite, f)。
            #[pymethod]
            fn set_scale(&self, sprite: PyRef<PySprite>, f: f32, vm: &VirtualMachine) -> PyResult<()> {
                let id = id_of(&sprite, vm)?;
                self.inner.borrow_mut().set_scale(id, f);
                Ok(())
            }

            /// set_visible(sprite, b)。
            #[pymethod]
            fn set_visible(&self, sprite: PyRef<PySprite>, b: bool, vm: &VirtualMachine) -> PyResult<()> {
                let id = id_of(&sprite, vm)?;
                self.inner.borrow_mut().set_visible(id, b);
                Ok(())
            }

            /// with_tag(tag) -> [Sprite]：标签过滤（返回存活句柄列表）。
            #[pymethod]
            fn with_tag(&self, tag: PyStrRef, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
                let t = tag
                    .to_str()
                    .ok_or_else(|| vm.new_value_error("tag 含非 UTF-8 字符"))?;
                let _ = vm;
                // v1：id 级过滤无法反查 Python 对象——返回数量（句柄反查
                // 属对象表设计，随 v2 资源句柄表立项）
                let n = self.inner.borrow().with_tag(t).len();
                Ok(vm.ctx.new_int(n as i64).into())
            }

            /// tick(dt_seconds)：推进帧动画/时序。
            #[pymethod]
            fn tick(&self, dt: f32) {
                self.inner.borrow_mut().tick(dt);
            }

            /// draw()：全组绘制到**活跃会话**（单形态定约——须在
            /// `with screen.render() as r:` 块内调用）。
            #[pymethod]
            fn draw(&self, vm: &VirtualMachine) -> PyResult<()> {
                crate::rpy::bridge::with_session(vm, |p| self.inner.borrow().draw(p))
            }
        }

        impl Constructor for PyGroup {
            type Args = ();

            fn py_new(
                _cls: &Py<PyType>,
                _args: Self::Args,
                _vm: &VirtualMachine,
            ) -> PyResult<Self> {
                Ok(Self { inner: std::cell::RefCell::new(sf_sprite::Group::new()) })
            }
        }
    }

    // ── egui：egui-starfish-we 原生风格绑定面（v1，2026-10-24）────
    // OOP 门面 `t.draw_ui(panel)`（会话方法，见 PyScreenSession）+
    // 生命周期面（init/capture/wants_*/shutdown）；实现本体在
    // crate::rpy::pyegui（帧序映射/指针模式/capture 定约见其文件头）。
    // feature "gui" 门控。

pub(crate) use sprite::module_def;
