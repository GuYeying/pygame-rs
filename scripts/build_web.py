#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""pygame-rs Web 构建脚本（wasm32 构建 + wasm-bindgen 胶水 + HTML 加载页）。

与 build_apk.py 同风格的 Web 侧统一入口——每个示例各自产物，互不覆盖，
逐一在 pygame-rs/web/ 下测试。

用法：
  python scripts/build_web.py binding_probe           # 窗口类示例（canvas）
  python scripts/build_web.py sig_probe --console     # 控制台类示例（输出在 F12）
  python scripts/build_web.py --all                   # 清单内全部构建

产物：pygame-rs/web/<名>.html + <名>.js + <名>_bg.wasm
测试：cd web && python -m http.server 8000
      浏览器打开 http://localhost:8000/<名>.html
      （file:// 直接打开无效——浏览器禁止 file 协议 fetch wasm）

前置：rustup target add wasm32-unknown-unknown
      wasm-bindgen-cli 版本必须与 Cargo.lock 严格一致（见 Cargo.lock 的
      wasm-bindgen 行；不一致报 "not compatible with version of the
      wasm-bindgen in the dependency tree"）。
"""

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WEB = ROOT / "web"
TARGET_DIR = ROOT / "target" / "wasm32-unknown-unknown" / "release" / "examples"
# Web CJK 字体源（fetch 预载用：testhost 的 preload_cjk 经 starfish_io::read
# 走 fetch "fonts/cjk.ttf"——ZhaoHua：5.1MB / 6767 CJK 字形）
CJK_FONT_SRC = ROOT / "resources" / "ZhaoHua.ttf"

# 注册清单：名 →（是否窗口类，页面标题说明）
REGISTRY = {
    "binding_probe": (True, "M1 绑定面探针：标准 pygame 写法（原生 pygame 模块 + with 会话渲染；Rect 直通 + typed K_* + font + mixer）"),
    "sig_probe": (False, "签名契约锚点：52 项可执行规格（结果在浏览器 F12 控制台）"),
    "rp_hello": (False, "M0 嵌入冒烟：RP 创世纪 / 语言特性 / 冻结 stdlib（结果在浏览器 F12 控制台）"),
    "font_test": (True, "pygame.font 专项测试：构造族/度量/render 一等化/CJK 链路逐项断言 + 字体样张（[font-test] 行）"),
    "transparent_test": (True, "透明窗口专项验证：桌宠形态基座（TRANSPARENT 创建 + 透明合成 + set_opacity 呼吸；[transparent-test] 行）"),
    "draw_test": (True, "pygame.draw 专项测试：六形状 × 填充/描边 × 颜色形态 × 半透明 + gfxdraw 兼容混绘（[draw-test] 行）"),
    "rapier_test": (True, "pygame_physics 2D 专项：落体调试线叠加通道（正交相机，[rapier-test] 行）"),
    "rapier3d_test": (True, "pygame_physics 3D 专项：3D 落体调试线（look-at 透视相机 + 深度通道，[rapier3d-test] 行）"),
    "egui_test": (True, "pygame_gui 专项：OOP 门面 HUD + 词汇 v2 控件（[egui-test] 行）"),
    "pip_test": (True, "画中画/游戏内面板：Surface 会话客人叠加（3D 物理动画 + 内嵌面板，[pip-test] 行）"),
    "parity_test": (True, "绑定收官 parity：颜色表/色彩空间/mixer Channel+Effect/深度会话（[parity-test] 行）"),
}

# 特例 features（缺省 = default features：gui + physics2d）
FEATURES = {
    "rapier3d_test": "physics3d",
    "pip_test": "physics3d",
}

CANVAS_TEMPLATE = """<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>pygame-rs · {name}</title>
<style>
  html, body {{ margin: 0; height: 100%; background: #000;
    display: flex; flex-direction: column;
    align-items: center; justify-content: center; }}
  /* ── 窗口大小控制（页面即权威，开发者在此调整）──
     canvas 占多大（CSS），渲染就是多大：引擎经 ResizeObserver 感知
     布局变化并自动同步位图（位图 = CSS 布局 1:1）+ 触发 rescale。
     默认 800×600（与桌面 set_mode 一致）。常用改法：
       • 铺满视口：width: 92vw; height: 86vh;
       • 跟随窗口：width: 100%; height: 100%（父级需有尺寸）
     运行时改大小见下方 <script> 里的 setGameSize 助手（默认注释，
     按需启用）。screen.get_size() 可在脚本里查询当前渲染尺寸。 */
  canvas {{ outline: none; width: 800px; height: 600px; }}
  p {{ color: #888; font: 13px/1.6 sans-serif; margin: 8px 0 0; }}
  /* 页面活体指示：浏览器 RAF 帧计数（纯 DOM，与 WebGL 无关）——
     数字在跳 = 页面渲染循环活着；数字停 = 整页死；数字跳而画布黑
     = WebGL canvas 合成问题（三态判据） */
  #raf {{ position: fixed; right: 8px; top: 6px; z-index: 20;
    color: #6c6; font: 11px/1 Consolas, monospace; opacity: .8; }}
  /* 页面内日志栏：console.* 转写（F12 不开也能看 print/异常——
     远程真机 / 自动化诊断用；上限 200 行滚动） */
  #wlog {{ width: 800px; max-height: 120px; overflow-y: auto; margin: 6px 0 0;
    padding: 4px 8px; box-sizing: border-box; white-space: pre-wrap;
    color: #9c9; background: #111; border: 1px solid #222; border-radius: 4px;
    font: 11px/1.5 Consolas, monospace; text-align: left; }}
  #start-overlay {{ position: fixed; inset: 0; background: rgba(0,0,0,.88);
    display: flex; align-items: center; justify-content: center;
    z-index: 10; }}
  #start-btn {{ padding: 14px 42px; font: 16px/1 sans-serif;
    color: #eee; background: #222; border: 1px solid #555;
    border-radius: 8px; cursor: pointer; }}
  #start-btn:hover {{ background: #333; border-color: #888; }}
</style>
</head>
<body>
<canvas id="starfish-we"></canvas>
<div id="raf">raf 0</div>
<p>【pygame-rs】{caption}</p>
<pre id="wlog" aria-label="console log"></pre>
<div id="start-overlay"><button id="start-btn">▶ 点击启动</button></div>
<script type="module">
// ── 页面活体指示（浏览器 RAF 帧计数；纯 DOM）──────────────────
(() => {{
  let n = 0;
  const el = document.getElementById('raf');
  const tick = () => {{ n++; if ((n % 10) === 0) el.textContent = 'raf ' + n;
                       requestAnimationFrame(tick); }};
  requestAnimationFrame(tick);
}})();

// ── console 转写（先于 wasm 加载就位；glue 每次调用都查 console.* 属性）──
const _wl = document.getElementById('wlog');
const _fmt = a => a.map(x => {{ try {{
  return (x && x.message !== undefined) ? x.message : String(x);
}} catch (e) {{ return '<unprintable>'; }} }}).join(' ');
for (const m of ['log', 'info', 'warn', 'error']) {{
  const orig = console[m].bind(console);
  console[m] = (...a) => {{
    _wl.textContent += '[' + m + '] ' + _fmt(a) + '\\n';
    const lines = _wl.textContent.split('\\n');
    if (lines.length > 200) _wl.textContent = lines.slice(-200).join('\\n');
    _wl.scrollTop = _wl.scrollHeight;
    orig(...a);
  }};
}}
window.addEventListener('error', e => {{
  _wl.textContent += '[onerror] ' + (e.message || e.error) +
    (e.filename ? ' @ ' + e.filename + ':' + e.lineno : '') + '\\n';
}});
window.addEventListener('unhandledrejection', e => {{
  _wl.textContent += '[rejection] ' + e.reason + '\\n';
}});

import init from './{name}.js?v={build_ts}';
// 点击启动 = 浏览器自动播放策略的部署约定：wasm（含 AudioContext
// 创建）整体落在首次用户手势调用栈之后，三平台行为一致
document.getElementById('start-btn').addEventListener('click', () => {{
  document.getElementById('start-overlay').remove();
  init().catch(e => {{ document.body.textContent = 'wasm 加载失败: ' + e; console.error(e); }});
}});

// ── 窗口大小控制（运行时）——按需取消注释启用 ──────────────────
// canvas 占多大（CSS）渲染就多大：改 style 后引擎经 ResizeObserver
// 自动同步位图并 rescale（screen.get_size() 可在脚本里查询当前尺寸）。
// window.setGameSize = (w, h) => {{
//   const c = document.getElementById('starfish-we');
//   c.style.width = w + 'px';
//   c.style.height = h + 'px';
// }};
// 用法（页面控制台或你的 JS 里）：setGameSize(1024, 768)
// 也可在游戏脚本侧配合：screen.get_size() 读取当前渲染尺寸做自适应。
</script>
</body>
</html>
"""

CONSOLE_TEMPLATE = """<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>pygame-rs · {name}</title>
<style>
  html, body {{ margin: 0; height: 100%; background: #000; color: #ccc;
    font: 14px/1.7 monospace; display: flex; flex-direction: column;
    align-items: center; justify-content: center; text-align: center; }}
  #start-overlay {{ position: fixed; inset: 0; background: rgba(0,0,0,.88);
    display: flex; align-items: center; justify-content: center;
    z-index: 10; }}
  #start-btn {{ padding: 14px 42px; font: 16px/1 monospace;
    color: #eee; background: #222; border: 1px solid #555;
    border-radius: 8px; cursor: pointer; }}
  #start-btn:hover {{ background: #333; border-color: #888; }}
</style>
</head>
<body>
<div>
  <p>【pygame-rs】{caption}</p>
  <p>输出在浏览器控制台：<b>F12 → Console</b>（Rust print 重定向到 console）</p>
</div>
<div id="start-overlay"><button id="start-btn">▶ 点击启动</button></div>
<script type="module">
import init from './{name}.js?v={build_ts}';
// 点击启动 = 浏览器自动播放策略的部署约定（同 canvas 页）
document.getElementById('start-btn').addEventListener('click', () => {{
  document.getElementById('start-overlay').remove();
  init().catch(e => {{ document.body.textContent = 'wasm 加载失败: ' + e; console.error(e); }});
}});
</script>
</body>
</html>
"""


def sh(cmd, tail=0):
    print("+", " ".join(str(c) for c in cmd))
    r = subprocess.run(cmd, capture_output=True, text=True, errors="replace")
    if r.returncode != 0:
        print(r.stdout[-3000:])
        print(r.stderr[-3000:])
        sys.exit(f"[FAIL] 命令失败: {cmd[0]} …")
    if tail:
        print("\n".join(r.stdout.splitlines()[-tail:]))
    return r.stdout


def check_bindgen_version():
    """wasm-bindgen-cli 必须与 Cargo.lock 的 wasm-bindgen 严格一致。"""
    r = subprocess.run(["wasm-bindgen", "--version"], capture_output=True, text=True)
    m_cli = re.search(r"(\d+\.\d+\.\d+)", r.stdout)
    cli = m_cli.group(1) if m_cli else None
    lock = ROOT / "Cargo.lock"
    lock_ver = None
    if lock.is_file():
        m_lock = re.search(r'name = "wasm-bindgen"\nversion = "([^"]+)"', lock.read_text(encoding="utf-8"))
        lock_ver = m_lock.group(1) if m_lock else None
    print(f"[版本] wasm-bindgen-cli = {cli} / Cargo.lock = {lock_ver}")
    if cli != lock_ver:
        sys.exit(f"[FAIL] 版本不一致（cli {cli} ≠ lock {lock_ver}）——重装："
                 f"cargo install wasm-bindgen-cli --version {lock_ver}")


def build_one(name, windowed, build_ts):
    caption = REGISTRY[name][1]
    print(f"=== 构建 {name}（{'canvas' if windowed else 'console'}）===")
    feats = FEATURES.get(name)
    feat_args = ["--features", feats] if feats else []
    sh(["cargo", "build", "--release", "--target", "wasm32-unknown-unknown",
        "--example", name, *feat_args])
    wasm = TARGET_DIR / f"{name}.wasm"
    if not wasm.is_file():
        sys.exit(f"[FAIL] wasm 产物缺失: {wasm}")
    sh(["wasm-bindgen", "--out-dir", str(WEB), "--target", "web", str(wasm)])

    tpl = CANVAS_TEMPLATE if windowed else CONSOLE_TEMPLATE
    (WEB / f"{name}.html").write_text(
        tpl.format(name=name, caption=caption, build_ts=build_ts), encoding="utf-8")
    print(f"[OK] web/{name}.html + 胶水 + wasm\n")


def main():
    ap = argparse.ArgumentParser(description="pygame-rs Web 构建脚本")
    ap.add_argument("names", nargs="*", help="示例名（见 REGISTRY）")
    ap.add_argument("--all", action="store_true", help="构建清单内全部示例")
    ap.add_argument("--console", action="store_true",
                    help="覆盖：按控制台页生成（默认按 REGISTRY 的窗口/控制台分类）")
    args = ap.parse_args()

    names = list(REGISTRY) if args.all else args.names
    if not names:
        ap.error("未指定示例名（可选：" + ", ".join(REGISTRY) + " 或 --all）")
    for n in names:
        if n not in REGISTRY:
            sys.exit(f"[FAIL] 未注册的示例: {n}（如需新增，往 REGISTRY 加一行）")

    WEB.mkdir(parents=True, exist_ok=True)
    # CJK 字体部署（fetch 预载目标 fonts/cjk.ttf；存在即跳过——
    # 5MB 字体不必每次重拷）
    fonts_dir = WEB / "fonts"
    fonts_dir.mkdir(exist_ok=True)
    cjk_dst = fonts_dir / "cjk.ttf"
    if not cjk_dst.is_file():
        if CJK_FONT_SRC.is_file():
            shutil.copy2(CJK_FONT_SRC, cjk_dst)
            print(f"[OK] CJK 字体部署: web/fonts/cjk.ttf（{cjk_dst.stat().st_size // 1048576}MB）")
        else:
            print("[WARN] CJK 字体源缺失（resources/ZhaoHua.ttf）——Web 中文将缺字")
    check_bindgen_version()
    from datetime import datetime
    build_ts = datetime.now().strftime("%Y%m%d%H%M%S")
    for n in names:
        windowed = not args.console if args.console else REGISTRY[n][0]
        build_one(n, windowed, build_ts)

    print("=== 测试方式 ===")
    print(f"cd {WEB}")
    print("python -m http.server 8000")
    print("浏览器打开 http://localhost:8000/<名>.html（file:// 无效）")


if __name__ == "__main__":
    main()
