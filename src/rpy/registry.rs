//! 绑定面模块注册（共享入口，2026-10-24 架构抽象）。
//!
//! **每顶层 Python 模块 = rpy 下一文件夹**（pygame / pygame_gui /
//! pygame_physics）；此处按依赖序统一挂载。**新增模块三步**：①rpy 下建
//! 文件夹（照抄 pygame_gui：mod.rs 放 #[pymodule] 声明 + 类型，
//! runtime.rs 放实现本体）；②rpy/mod.rs 声明 + Cargo feature 按需；
//! ③本文件 `try_install` 末尾补一行 `mount_top`。
//!
//! 挂载序定约：
//! 1. pygame 先挂（顶层类型 Surface/Color 等——gui/physics 的会话协作
//!    面在**调用期**引用其 static type，挂载期零依赖）；
//! 2. pygame 子模块按依赖序，**event 先于 key**——key 的 K_* 挂载钩子
//!    （module_exec）取 PyKey 的 static type，PyKey 在 event 的
//!    `__init_attributes` 里创建（"static type has not been initialized"
//!    实测教训）；
//! 3. pygame_gui / pygame_physics 作为独立顶层模块随后
//!    （Python 面 `import pygame_gui` / `import pygame_physics` 直达，
//!    不再是 pygame 子模块——rapier/egui 与 pygame 无 import 耦合）；
//! 4. 全部顶层模块 + pygame 常用子模块补进 sys.modules。

use rustpython_vm::{PyObjectRef, VirtualMachine, builtins::PyModuleDef};

use crate::rpy::bridge::exc_msg;
use crate::rpy::pygame;
#[cfg(feature = "gui")]
use crate::rpy::pygame_gui;
#[cfg(any(feature = "physics2d", feature = "physics3d"))]
use crate::rpy::pygame_physics;

/// 多阶段 init 后注入 sys.modules（宿主壳统一入口）。
pub fn install(vm: &VirtualMachine) {
    // 颜色表装载（`Color("name")` 命名构造依赖；双 init 错误静默——
    // install 本就单次，防御性容忍）
    let _ = starfish_pygame::color::init_colors(starfish_pygame::COLORS_JSON, true);
    if let Err(msg) = try_install(vm) {
        panic!("绑定面模块注入失败: {msg}");
    }
}

fn try_install(vm: &VirtualMachine) -> Result<(), String> {
    let sys_modules = vm
        .sys_module
        .get_attr("modules", vm)
        .map_err(|e| format!("sys.modules fetch: {}", exc_msg(vm, e)))?;

    // ── pygame 主模块（子模块挂在其属性下）──
    let pygame_mod = mount_top(vm, &sys_modules, pygame::pygame::module_def(&vm.ctx), "pygame")?;
    let children: &[Child] = &[
        child("display", pygame::display::module_def(&vm.ctx)),
        child("time", pygame::time::module_def(&vm.ctx)),
        child("font", pygame::font::module_def(&vm.ctx)),
        child("mixer", pygame::mixer::module_def(&vm.ctx)),
        child("math", pygame::math::module_def(&vm.ctx)),
        child("sndarray", pygame::sndarray::module_def(&vm.ctx)),
        child("version", pygame::version::module_def(&vm.ctx)),
        child("transform", pygame::transform::module_def(&vm.ctx)),
        child("image", pygame::image::module_def(&vm.ctx)),
        // event 先于 key：key 的 K_* 挂载钩子（module_exec）取 PyKey 的
        // static type——PyKey 类型在 event 的 __init_attributes 里创建。
        child("event", pygame::event::module_def(&vm.ctx)),
        child("key", pygame::key::module_def(&vm.ctx)),
        child("mouse", pygame::mouse::module_def(&vm.ctx)),
        child("gamepad", pygame::gamepad::module_def(&vm.ctx)),
        child("touch", pygame::touch::module_def(&vm.ctx)),
        child("io", pygame::io::module_def(&vm.ctx)),
        child("net", pygame::net::module_def(&vm.ctx)),
        child("dialog", pygame::dialog::module_def(&vm.ctx)),
        child("gfxdraw", pygame::gfxdraw::module_def(&vm.ctx)),
        child("render", pygame::render::module_def(&vm.ctx)),
        child("mask", pygame::mask::module_def(&vm.ctx)),
        child("permission", pygame::permission::module_def(&vm.ctx)),
        child("video", pygame::video::module_def(&vm.ctx)),
        child("sprite", pygame::sprite::module_def(&vm.ctx)),
    ];
    for c in children {
        mount_child(vm, &pygame_mod, &sys_modules, c)?;
    }

    // ── 独立顶层模块（新增模块在此照抄一行）──
    #[cfg(feature = "gui")]
    mount_top(vm, &sys_modules, pygame_gui::module_def(&vm.ctx), "pygame_gui")?;
    // pygame_physics = 双维度单模块（2D 类面 physics2d + 3D 类面
    // physics3d）——任一维度开启即挂载
    #[cfg(any(feature = "physics2d", feature = "physics3d"))]
    mount_top(
        vm,
        &sys_modules,
        pygame_physics::module_def(&vm.ctx),
        "pygame_physics",
    )?;
    Ok(())
}

/// 子模块挂载描述（attr = 父模块属性名；def = 各绑定文件的 def）
struct Child {
    attr: &'static str,
    full: &'static str,
    def: &'static PyModuleDef,
}

fn child(attr: &'static str, def: &'static PyModuleDef) -> Child {
    Child {
        attr,
        full: match attr {
            "display" => "pygame.display",
            "time" => "pygame.time",
            "font" => "pygame.font",
            "mixer" => "pygame.mixer",
            "math" => "pygame.math",
            "sndarray" => "pygame.sndarray",
            "version" => "pygame.version",
            "transform" => "pygame.transform",
            "image" => "pygame.image",
            "key" => "pygame.key",
            "mouse" => "pygame.mouse",
            "gamepad" => "pygame.gamepad",
            "touch" => "pygame.touch",
            "io" => "pygame.io",
            "net" => "pygame.net",
            "dialog" => "pygame.dialog",
            "gfxdraw" => "pygame.gfxdraw",
            "mask" => "pygame.mask",
            "permission" => "pygame.permission",
            "video" => "pygame.video",
            "sprite" => "pygame.sprite",
            _ => "pygame.event",
        },
        def,
    }
}

/// 顶层模块挂载：create + exec + sys.modules 直达
/// （`import <name>` 不经父模块——独立模块的既定形态）。
fn mount_top(
    vm: &VirtualMachine,
    sys_modules: &PyObjectRef,
    def: &'static PyModuleDef,
    name: &str,
) -> Result<PyObjectRef, String> {
    let module = def
        .create_module(vm)
        .map_err(|e| format!("create_module({name}): {}", exc_msg(vm, e)))?;
    def.exec_module(vm, &module)
        .map_err(|e| format!("exec_module({name}): {}", exc_msg(vm, e)))?;
    let obj: PyObjectRef = module.into();
    sys_modules
        .set_item(name, obj.clone(), vm)
        .map_err(|e| format!("sys.modules inject {name}: {}", exc_msg(vm, e)))?;
    Ok(obj)
}

/// pygame 子模块挂载：create + exec + 父属性 + sys.modules 补账
/// （`import pygame.xxx` 直达；其余子模块靠属性访问，行为与拆分前一致）。
fn mount_child(
    vm: &VirtualMachine,
    parent: &PyObjectRef,
    sys_modules: &PyObjectRef,
    c: &Child,
) -> Result<(), String> {
    let child = c
        .def
        .create_module(vm)
        .map_err(|e| format!("child {}: {}", c.attr, exc_msg(vm, e)))?;
    c.def
        .exec_module(vm, &child)
        .map_err(|e| format!("child {}: {}", c.attr, exc_msg(vm, e)))?;
    let child_obj: PyObjectRef = child.into();
    parent
        .set_attr(c.attr, child_obj.clone(), vm)
        .map_err(|e| format!("child {}: {}", c.attr, exc_msg(vm, e)))?;
    let in_sys = matches!(
        c.full,
        "pygame.display"
            | "pygame.event"
            | "pygame.time"
            | "pygame.font"
            | "pygame.mixer"
            | "pygame.transform"
            | "pygame.image"
            | "pygame.mask"
            | "pygame.permission"
            | "pygame.video"
    );
    if in_sys {
        sys_modules
            .set_item(c.full, child_obj, vm)
            .map_err(|e| format!("child inject {}: {}", c.full, exc_msg(vm, e)))?;
    }
    Ok(())
}
