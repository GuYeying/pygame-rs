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
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WEB = ROOT / "web"
TARGET_DIR = ROOT / "target" / "wasm32-unknown-unknown" / "release" / "examples"

# 注册清单：名 →（是否窗口类，页面标题说明）
REGISTRY = {
    "binding_probe": (True, "M1 绑定面探针：标准 pygame 写法（原生 pygame 模块 + with 会话渲染；Rect 直通 + typed K_* + font + mixer）"),
    "sig_probe": (False, "签名契约锚点：52 项可执行规格（结果在浏览器 F12 控制台）"),
    "rp_hello": (False, "M0 嵌入冒烟：RP 创世纪 / 语言特性 / 冻结 stdlib（结果在浏览器 F12 控制台）"),
    "font_test": (True, "pygame.font 专项测试：构造族/度量/render 一等化/CJK 链路逐项断言 + 字体样张（[font-test] 行）"),
    "transparent_test": (True, "透明窗口专项验证：桌宠形态基座（TRANSPARENT 创建 + 透明合成 + set_opacity 呼吸；[transparent-test] 行）"),
    "draw_test": (True, "pygame.draw 专项测试：六形状 × 填充/描边 × 颜色形态 × 半透明 + gfxdraw 兼容混绘（[draw-test] 行）"),
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
  canvas {{ outline: none; }}
  p {{ color: #888; font: 13px/1.6 sans-serif; margin: 8px 0 0; }}
</style>
</head>
<body>
<canvas id="starfish-window" width="800" height="600"></canvas>
<p>【pygame-rs】{caption}</p>
<script type="module">
import init from './{name}.js?v={build_ts}';
init().catch(e => {{ document.body.textContent = 'wasm 加载失败: ' + e; console.error(e); }});
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
</style>
</head>
<body>
<div>
  <p>【pygame-rs】{caption}</p>
  <p>输出在浏览器控制台：<b>F12 → Console</b>（Rust print 重定向到 console）</p>
</div>
<script type="module">
import init from './{name}.js?v={build_ts}';
init().catch(e => {{ document.body.textContent = 'wasm 加载失败: ' + e; console.error(e); }});
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
    sh(["cargo", "build", "--release", "--target", "wasm32-unknown-unknown",
        "--example", name])
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
