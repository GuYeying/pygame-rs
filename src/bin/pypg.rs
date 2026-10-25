//! pypg —— **开发解释器**（"pygame 的 python"，产品化批次 58）：开发者
//! 日常工具，pygame 全内置、零 Rust 工具链。与 pygame-runner（分发
//! 载体，只认游戏门）分工——见 reference/开发与游戏打包体系设计稿。
//!
//! 四种调用形态：
//! - `pypg`                          → REPL（交互式；quit()/Ctrl-C 退）
//! - `pypg -c "<代码>"`              → 单行直跑
//! - `pypg <脚本.py> [参数...]`      → 脚本直跑（任意脚本）
//! - `pypg --game <目录>`             → 游戏门（game.toml entry +
//!   async def main() 协程；同 pygame-runner）
//!
//! **入口定约（批次 59 归一化）**：游戏门脚本严格 `async def main():`
//!（帧体 `await pygame.next_frame()`）；脚本直跑形态无入口约束
//!（顶层同步执行至结束）。

use pygame_rs::rpy;

fn format_tb(
    vm: &rustpython_vm::VirtualMachine,
    exc: &rustpython_vm::builtins::PyBaseExceptionRef,
) -> String {
    let mut buf = std::io::Cursor::new(Vec::<u8>::new());
    let _ = vm.write_exception(&mut rustpython_vm::py_io::IoWriter(&mut buf), exc);
    String::from_utf8_lossy(buf.get_ref()).trim_end().to_string()
}

/// 装配（解释器 enter 内 stdio 接线 + 模块注入）——返回 globals 供
/// 各形态 run_code_obj 使用
fn enter_setup(interp: &rustpython_vm::Interpreter) -> rustpython_vm::scope::Scope {
    let scope = std::cell::RefCell::new(None);
    interp.enter(|vm| {
        let sc = vm.new_scope_with_builtins();
        rpy::install_modules(vm);
        *scope.borrow_mut() = Some(sc);
    });
    scope.into_inner().expect("enter 必然装配 scope")
}

/// 编译 + 错误统一 traceback 化（须在 enter 内调用——依赖 vm）
fn vm_compile(
    interp: &rustpython_vm::Interpreter,
    source: &str,
    name: &str,
) -> Option<rustpython_vm::PyRef<rustpython_vm::builtins::PyCode>> {
    interp.enter(|vm| {
        match vm.compile(
            source,
            rustpython_vm::compiler::Mode::Exec,
            name.to_string(),
        ) {
            Ok(c) => Some(c),
            Err(e) => {
                let exc = e.into_pyexception(vm, Some(name));
                eprintln!("[pypg] 脚本编译异常:\n{}", format_tb(vm, &exc));
                None
            }
        }
    })
}

fn usage() -> ! {
    eprintln!(
        "pypg —— pygame 的 python（开发解释器）\n\
\n\
用法：\n\
  pypg                            交互式 REPL\n\
  pypg -c \"<代码>\"                单行直跑\n\
  pypg <脚本.py> [参数...]         脚本直跑（任意脚本）\n\
  pypg --game <目录>               游戏门（game.toml entry + async def main()）\n\
\n\
pygame 全内置（import pygame 即用）；脚本不触渲染则完全不依赖 GPU。"
    );
    std::process::exit(2);
}

fn main() {
    // 直接调 block_on（不用 app_entry! 宏——pypg 纯桌面工具，宏的
    // Android/Web 入口分支与尾部 exit(0) 是死代码/退出码干扰）
    starfish_std::app::block_on(async move {
        let mut argv = std::env::args().skip(1);
        let first = match argv.next() {
            Some(a) => a,
            None => repl_mode(),
        };
        match first.as_str() {
            "-c" => {
                let code = match argv.next() {
                    Some(c) => c,
                    None => usage(),
                };
                let interp = rpy::new_interpreter();
                let scope = enter_setup(&interp);
                match vm_compile(&interp, &code, "<pypg -c>") {
                    Some(c) => {
                        if let Err(e) = interp.enter(|vm| {
                            vm.run_code_obj(c, scope.clone()).map_err(|e| {
                                format!("脚本执行异常:\n{}", format_tb(vm, &e))
                            })
                        }) {
                            eprintln!("[pypg] {e}");
                            std::process::exit(1);
                        }
                    }
                    None => std::process::exit(1),
                }
            }
            "--game" => {
                let dir = match argv.next() {
                    Some(d) => d,
                    None => usage(),
                };
                run_game(&dir).await;
                return;
            }
            script_path => {
                let script = match std::fs::read_to_string(&script_path) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("[pypg] 无法读取 {script_path}: {e}");
                        std::process::exit(1);
                    }
                };
                let interp = rpy::new_interpreter();
                let scope = enter_setup(&interp);
                match vm_compile(&interp, &script, &script_path) {
                    Some(c) => {
                        if let Err(e) = interp.enter(|vm| {
                            vm.run_code_obj(c, scope.clone()).map_err(|e| {
                                format!("脚本执行异常:\n{}", format_tb(vm, &e))
                            })
                        }) {
                            eprintln!("[pypg] {e}");
                            std::process::exit(1);
                        }
                    }
                    None => std::process::exit(1),
                }
            }
        }
    });
}

/// 游戏门（同 pygame-runner）：entry 文件 + async def main() 协程
async fn run_game(arg: &str) {
    // canonicalize 先行：相对路径的 parent() 可能为空导致目录误判
    let p = std::path::PathBuf::from(arg)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(arg));
    let (game_dir, default_entry) = if p.extension().and_then(|e| e.to_str()) == Some("py") {
        // 单文件形态：父目录为游戏目录、文件名为入口
        let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
        (dir, p.file_name().unwrap().to_string_lossy().to_string())
    } else {
        (p.clone(), "main.py".to_string())
    };
    if !game_dir.is_dir() {
        eprintln!("[pypg] 游戏目录不存在: {}", game_dir.display());
        std::process::exit(1);
    }
    std::env::set_current_dir(&game_dir).expect("切换游戏目录失败");
    let entry = std::fs::read_to_string("game.toml")
        .ok()
        .and_then(|toml| {
            toml.lines().find_map(|l| {
                let l = l.trim();
                l.strip_prefix("entry")?
                    .trim_start()
                    .strip_prefix('=')?
                    .trim()
                    .split('#')
                    .next()
                    .map(|v| v.trim().trim_matches(['"', '\'']).to_string())
            })
        })
        .unwrap_or(default_entry);
    let script = std::fs::read_to_string(&entry).expect("读取入口文件失败");
    println!("[pypg] 入口 {entry}（{} 字节），启动", script.len());
    if let Err(e) = rpy::testhost::run(&script, "pypg_game", false).await {
        eprintln!("[pypg] 退出: {e}");
        std::process::exit(1);
    }
}


/// REPL（逐行；表达式值回显）
fn repl_mode() -> ! {
    println!("pypg REPL（pygame 全内置；quit() 退出）");
    let interp = rpy::new_interpreter();
    let scope = enter_setup(&interp);
    let stdin = std::io::stdin();
    loop {
        print!(">>> ");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if stdin.read_line(&mut line).unwrap_or(0) == 0 {
            break; // EOF
        }
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if line == "quit()" || line == "exit()" {
            break;
        }
        // 表达式优先（Single 语义：值回显）；非表达式降级 Exec
        let eval = vm_compile_eval(&interp, line);
        match eval {
            Some(c) => {
                interp.enter(|vm| {
                    match vm.run_code_obj(c, scope.clone()) {
                        Ok(v) => {
                            if !vm.is_none(&v) {
                                if let Ok(r) = v.repr(vm) {
                                    println!("{}", r);
                                }
                            }
                        }
                        Err(e) => eprintln!("{}", format_tb(vm, &e)),
                    }
                });
            }
            None => {
                match vm_compile_exec(&interp, line) {
                    Some(c) => {
                        interp.enter(|vm| {
                            if let Err(e) = vm.run_code_obj(c, scope.clone()) {
                                eprintln!("{}", format_tb(vm, &e));
                            }
                        });
                    }
                    None => eprintln!("SyntaxError: 语句编译失败"),
                }
            }
        }
    }
    std::process::exit(0);
}

/// Single 模式编译（REPL 值回显用）
fn vm_compile_eval(
    interp: &rustpython_vm::Interpreter,
    source: &str,
) -> Option<rustpython_vm::PyRef<rustpython_vm::builtins::PyCode>> {
    interp.enter(|vm| {
        vm.compile(
            source,
            rustpython_vm::compiler::Mode::Eval,
            "<pypg repl>".to_string(),
        )
        .ok()
    })
}

/// Exec 模式编译（REPL 语句降级用）
fn vm_compile_exec(
    interp: &rustpython_vm::Interpreter,
    source: &str,
) -> Option<rustpython_vm::PyRef<rustpython_vm::builtins::PyCode>> {
    interp.enter(|vm| {
        vm.compile(
            source,
            rustpython_vm::compiler::Mode::Exec,
            "<pypg repl>".to_string(),
        )
        .ok()
    })
}
