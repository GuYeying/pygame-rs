//! 游戏脚本模块加载器（`sys.meta_path` 钩子，2026-10-26 定约）。
//!
//! **解决什么**：冻结 stdlib 的文件查找 machinery（`importlib._bootstrap_external`）
//! 依赖原生 `nt`/`posix` + `_io.open_code`——不可用（三平台同根因），
//! `import importlib.*` 亦被 `importlib/__init__` 的无条件引入拖死。
//! 本模块按 reference/实现模块加载的方案.md（方案 A）绕开：**自持
//! meta_path Finder + 内存模块表**，开发者写标准 `import xxx` 即触发。
//!
//! **已实测语义**（原型 P1-P6，详见方案验证）：普通模块 / 包
//! `__init__` / 相对导入（`from . import x`）/ from-import / sys.modules
//! 注册（循环导入安全）/ `__file__` 虚拟路径 + `__package__` / 未知名
//! 回落标准 `ModuleNotFoundError`。
//!
//! **两处 RP 冻结环境适配**（原型踩坑实证）：
//! 1. 规格协议从 `sys.modules["_frozen_importlib"]` 取（= 自举期已加载
//!    的 `_bootstrap` 本体；`import importlib` 走不通）；
//! 2. `spec.has_location` 不可开——`ModuleSpec.cached` 在无
//!    `_bootstrap_external` 时抛 NotImplementedError；`__file__` 由
//!    Loader 在 exec_module 内自行挂载。
//!
//! **跨平台定约**：import 同步、starfish-io 异步 → 导入热路径零 IO——
//! 宿主**启动期**建好内存模块表（桌面 [`scan_module_dir`] 同步扫盘；
//! 打包态 [`install_module_table`] 预载，Web 端 fetch 异步只在启动期），
//! Finder 纯内存查表。模块路径基准 = main 所在目录（宿主切 cwd 或传
//! 绝对根）；目录名由配置/约定给出（dev = `modules/`）。

use std::cell::RefCell;
use std::collections::BTreeMap;

use rustpython_derive::pymodule;
use rustpython_vm::{
    PyObjectRef, VirtualMachine,
    builtins::PyModuleDef,
};

use crate::rpy::bridge::exc_msg;

/// 内存模块表：点分名 → (虚拟文件名, 源码, 是否包)
thread_local! {
    static TABLE: RefCell<BTreeMap<String, (String, String, bool)>> =
        RefCell::new(BTreeMap::new());
}

thread_local! {
    static FINDER_INSTALLED: RefCell<bool> = const { RefCell::new(false) };
}

/// 桌面扫描：递归收集 `root` 下全部 .py 进内存表。
/// 名字映射 = 相对路径（`/` 分隔，点分名）；`__init__.py` → 包
///（目录优先：同名 .py 与包并存时包胜——python 同语义）。
/// 返回装载条数；目录不存在 = 0。
#[cfg(not(target_arch = "wasm32"))]
pub fn scan_module_dir(root: &str) -> usize {
    let mut table = BTreeMap::new();
    let root_path = std::path::Path::new(root);
    if root_path.is_dir() {
        walk(root_path, "", "", &mut table);
    }
    let n = table.len();
    TABLE.with(|t| *t.borrow_mut() = table);
    n
}

/// Web 桩（无文件系统；打包态走 [`install_module_table`]）
#[cfg(target_arch = "wasm32")]
pub fn scan_module_dir(_root: &str) -> usize {
    0
}

/// 打包态直接装表（`(点分名, 虚拟文件名, 源码, 是否包)`）——清单由
/// 打包器生成、宿主经 starfish-io 启动期读入后调此口。
pub fn install_table(entries: Vec<(String, String, String, bool)>) -> usize {
    let mut table = BTreeMap::new();
    for (name, filename, src, is_pkg) in entries {
        table.insert(name, (filename, src, is_pkg));
    }
    let n = table.len();
    TABLE.with(|t| *t.borrow_mut() = table);
    n
}

#[cfg(not(target_arch = "wasm32"))]
fn walk(dir: &std::path::Path, path_prefix: &str, dot_prefix: &str, out: &mut BTreeMap<String, (String, String, bool)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    items.sort_by_key(|e| e.file_name());
    for entry in items {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "__pycache__" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            // 包目录：内含 __init__.py 才登记为包
            let init = path.join("__init__.py");
            if init.is_file() {
                if let Ok(src) = std::fs::read_to_string(&init) {
                    out.insert(
                        format!("{dot_prefix}{name}"),
                        (format!("{path_prefix}{name}/__init__.py"), src, true),
                    );
                }
            }
            walk(
                &path,
                &format!("{path_prefix}{name}/"),
                &format!("{dot_prefix}{name}."),
                out,
            );
        } else if let Some(base) = name.strip_suffix(".py") {
            if base == "__init__" {
                continue; // 已随包目录登记
            }
            let key = format!("{dot_prefix}{base}");
            // 包优先（python 同语义）：同名包已登记则跳过散 .py
            if out.get(&key).map(|(_, _, is_pkg)| *is_pkg) == Some(true) {
                continue;
            }
            if let Ok(src) = std::fs::read_to_string(&path) {
                out.insert(key, (format!("{path_prefix}{base}.py"), src, false));
            }
        }
    }
}

#[pymodule]
mod sf_loader {
    use rustpython_vm::{
        PyObjectRef, PyResult, VirtualMachine,
        builtins::PyStrRef,
    };

    /// read_module_source(name) -> (filename, src, is_pkg) | None
    ///（内存表查按需取源；None = 表中没有，Finder 回落标准 machinery）
    #[pyfunction]
    fn read_module_source(name: PyStrRef, vm: &VirtualMachine) -> PyResult<Option<PyObjectRef>> {
        let key = name.to_str().unwrap_or("");
        let hit = super::TABLE.with(|t| t.borrow().get(key).cloned());
        Ok(hit.map(|(filename, src, is_pkg)| {
            vm.ctx
                .new_tuple(vec![
                    vm.ctx.new_str(filename).into(),
                    vm.ctx.new_str(src).into(),
                    vm.ctx.new_bool(is_pkg).into(),
                ])
                .into()
        }))
    }
}

/// 注入 Finder（幂等——表可重装，Finder 只挂一次）
fn ensure_finder(vm: &VirtualMachine) -> Result<(), String> {
    if FINDER_INSTALLED.with(|f| *f.borrow()) {
        return Ok(());
    }
    // 内置函数模块挂进 sys.modules（finder 经作用域全局直达，无需 import）
    let sys_modules = vm
        .sys_module
        .get_attr("modules", vm)
        .map_err(|e| format!("sys.modules: {}", exc_msg(vm, e)))?;
    let def: &'static PyModuleDef = sf_loader::module_def(&vm.ctx);
    let module = def
        .create_module(vm)
        .and_then(|m| {
            def.exec_module(vm, &m)?;
            Ok(m)
        })
        .map_err(|e| format!("_sf_loader 装配: {}", exc_msg(vm, e)))?;
    let loader_mod: PyObjectRef = module.into();
    sys_modules
        .set_item("_sf_loader", loader_mod.clone(), vm)
        .map_err(|e| format!("sys.modules[_sf_loader]: {}", exc_msg(vm, e)))?;

    const FINDER_SRC: &str = r#"
import sys as _sys
_b = _sys.modules["_frozen_importlib"]
class SfLoader:
    def __init__(self, filename, src):
        self.filename = filename
        self.src = src
    def create_module(self, spec):
        return None
    def exec_module(self, module):
        module.__dict__["__file__"] = self.filename
        exec(compile(self.src, self.filename, "exec"), module.__dict__)
class SfFinder:
    def find_spec(self, fullname, path=None, target=None):
        ent = _sf_loader.read_module_source(fullname)
        if ent is None:
            return None
        filename, src, is_pkg = ent
        spec = _b.ModuleSpec(fullname, SfLoader(filename, src), origin=filename, is_package=is_pkg)
        if is_pkg:
            spec.submodule_search_locations = [filename.rsplit("/", 1)[0]]
        return spec
_sys.meta_path.insert(0, SfFinder())
"#;
    let code = vm
        .compile(
            FINDER_SRC,
            rustpython_vm::compiler::Mode::Exec,
            "<sf_module_finder>".to_string(),
        )
        .map_err(|e| {
            let exc = e.into_pyexception(vm, Some("<sf_module_finder>"));
            format!("finder 编译: {}", exc_msg(vm, exc))
        })?;
    let scope = vm.new_scope_with_builtins();
    scope
        .globals
        .set_item("_sf_loader", loader_mod, vm)
        .map_err(|e| format!("_sf_loader 全局注入: {}", exc_msg(vm, e)))?;
    vm.run_code_obj(code, scope)
        .map_err(|e| format!("finder 注入: {}", exc_msg(vm, e)))?;
    FINDER_INSTALLED.with(|f| *f.borrow_mut() = true);
    Ok(())
}

/// 宿主入口：扫描 `dir`（相对 = main 所在目录，python 同语义）建表 +
/// 注入 Finder。返回装载条数（0 = 目录缺失，不做任何注入）。
pub fn install_module_finder(vm: &VirtualMachine, dir: &str) -> Result<usize, String> {
    let n = scan_module_dir(dir);
    if n > 0 {
        ensure_finder(vm)?;
    }
    Ok(n)
}

/// 宿主入口（打包态）：直接装表 + 注入 Finder。
pub fn install_module_finder_with_table(
    vm: &VirtualMachine,
    entries: Vec<(String, String, String, bool)>,
) -> Result<usize, String> {
    let n = install_table(entries);
    if n > 0 {
        ensure_finder(vm)?;
    }
    Ok(n)
}
