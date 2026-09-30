//! M0 嵌入冒烟：解释器创世纪 → 编译 → 执行，覆盖绑定层依赖的全部
//! RP 语言特性（print / match 解构 / 生成器帧边界 / 冻结 stdlib 导入）。

use rustpython_vm::{Interpreter, Settings};
use rustpython_vm::compiler::Mode;

/// 冒烟脚本：覆盖绑定层依赖的全部特性面
const SOURCE: &str = r#"
print("hello from RustPython")

# match 解构（绑定层 Event 分发的基础）
match {"kind": "quit"}:
    case {"kind": "quit"}:
        print("match: quit")

# 生成器帧边界（with/帧步进的控制权交接原语）
def gen(n):
    for i in range(n):
        yield i * 2

print("gen:", list(gen(4)))

# 冻结 stdlib 导入（freeze-stdlib）
import json
print("json:", json.dumps({"ok": True}))
"#;

/// 跑通 RP：Err = 冒烟失败（含编译/Python 异常文本）。
pub fn run_smoke() -> Result<(), String> {
    let interp = Interpreter::builder(Settings::default())
        .init_hook(|vm| {
            // debug 构建默认 limit=256，frozen importlib 引导即撞线
            // （RecursionError）——抬到 release 同款 1000。
            // （Python 帧计数守卫；Windows debug 的 C 栈软限位撞线另由
            // build.rs /STACK 解决——两个守卫互相独立。）
            vm.recursion_limit.set(1000);
        })
        .add_frozen_modules(rustpython_pylib::FROZEN_STDLIB)
        .build();
    interp.enter(|vm| {
        let scope = vm.new_scope_with_builtins();
        let code = vm
            .compile(SOURCE, Mode::Exec, "smoke.py".to_owned())
            .map_err(|e| e.to_string())?;
        vm.run_code_obj(code, scope)
            .map_err(|e| format!("PyException: {e:?}"))?;
        Ok(())
    })
}
