//! pygame-runner —— 桌面通用游戏运行器（"python.exe" 形态，产品化
//! 批次）：一个 exe 跑任意游戏目录，而非每游戏编一个 example。
//! build_windows.py 打包链的目标产物。
//!
//! 入口约定（批次 58 定稿）：**严格 `def main():` 生成器门**（跨平台
//! 统一——Web rAF 模型的硬约束）。入口**文件**由配置指定：
//! 游戏目录的 `game.toml`（打包时写入；`[app] entry = "main.py"`
//! 缺省 main.py）——运行时按配置先加载该文件，调其中的 `main()`。
//!
//! 运行形态：
//! - `pygame-runner`（无参）：跑当前目录（game.toml / main.py）
//! - `pygame-runner <目录>`：跑 `<目录>/`（cwd 切过去保资源相对路径）

use pygame_rs::rpy;

starfish_std::app_entry!({
    let arg = std::env::args().nth(1);
    let game_dir = match arg {
        Some(d) => std::path::PathBuf::from(d),
        None => std::env::current_dir().expect("取当前目录失败"),
    };
    if !game_dir.is_dir() {
        eprintln!("[runner] 游戏目录不存在: {}", game_dir.display());
        std::process::exit(1);
    }
    // 绝对化先于 cwd 切换（join 相对 game_dir 会被切换后 cwd 二次解析）
    let game_dir = game_dir.canonicalize().expect("规范化游戏目录失败");
    std::env::set_current_dir(&game_dir).expect("切换游戏目录失败");

    // 入口文件：game.toml [app].entry（缺省 main.py）
    let entry = std::fs::read_to_string("game.toml")
        .ok()
        .and_then(|toml| {
            // 轻量行解析（pack_common mini 解析的 Rust 版——单键提取）
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
        .unwrap_or_else(|| "main.py".to_string());
    let main_py = game_dir.join(&entry);
    if !main_py.is_file() {
        eprintln!("[runner] 入口文件缺失: {}（game.toml entry={entry}）", main_py.display());
        std::process::exit(1);
    }
    let script = std::fs::read_to_string(&main_py).expect("读取入口文件失败");
    println!("[runner] 入口 {entry}（{} 字节），启动", script.len());
    if let Err(e) = rpy::testhost::run(&script, "game", false).await {
        eprintln!("[runner] 退出: {e}");
        std::process::exit(1);
    }
});
