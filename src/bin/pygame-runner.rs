//! pygame-runner —— 桌面通用游戏运行器（"python.exe" 形态，产品化
//! 批次）：一个 exe 跑任意游戏目录（`main.py` + 资源），而非每游戏
//! 编一个 example。build_windows.py 打包链的目标产物。
//!
//! 运行形态：
//! - `pygame-runner`（无参）：跑当前目录 `./main.py`
//! - `pygame-runner <目录>`：跑 `<目录>/main.py`
//! 工作目录切到游戏目录（资源的相对路径语义与脚本预期一致）。

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
    let main_py = game_dir.join("main.py");
    if !main_py.is_file() {
        eprintln!("[runner] 未找到 main.py: {}", main_py.display());
        std::process::exit(1);
    }
    // 工作目录 = 游戏目录（资源相对路径语义；fonts/cjk.ttf 等预载路径
    // 以 cwd 解析）
    std::env::set_current_dir(&game_dir).expect("切换游戏目录失败");
    let script = std::fs::read_to_string(&main_py).expect("读取 main.py 失败");
    println!("[runner] main.py {} 字节，启动", script.len());
    if let Err(e) = rpy::testhost::run(&script, "game", false).await {
        eprintln!("[runner] 退出: {e}");
        std::process::exit(1);
    }
});
