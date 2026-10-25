#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""pygame-rs Web 构建脚本（产品化批次 toml 化，2026-10-25）。

配置 = pack/<名>.web.toml（每案例一份；--init <名> 生成模板——
buildozer 风格分节、更细粒度：Web 平台认识的键 Web 平台用）。

用法：
    python scripts/build_web.py --init my_game   # 生成 pack/my_game.web.toml
    python scripts/build_web.py my_game [名...]  # 读 toml 打包
    python scripts/build_web.py --all            # pack/*.web.toml 全打

产物：pygame-rs/web/<名>.html + <名>.js + <名>_bg.wasm
测试：cd web && python -m http.server 8000
      浏览器打开 http://localhost:8000/<名>.html（file:// 无效）
前置：rustup target add wasm32-unknown-unknown；wasm-bindgen-cli 版本
      与 Cargo.lock 严格一致（脚本自检）。
"""

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ImportError:
    tomllib = None

ROOT = Path(__file__).resolve().parent.parent
WEB = ROOT / "web"
TARGET_DIR = ROOT / "target" / "wasm32-unknown-unknown" / "release" / "examples"
PACK_DIR = ROOT / "pack"
CJK_FONT_SRC = ROOT / "resources" / "ZhaoHua.ttf"

WEB_TEMPLATE = """# Web 打包配置（build_web.py --init 生成；可直接改）
[app]
name = "{name}"                # 案例名（examples/<name>.py + .rs）
windowed = true                 # true = canvas 窗口页 / false = 控制台页
caption = "{name}"             # 页面说明行

[build]
features = []                   # cargo features（physics3d 等）

[assets]
fonts = ["resources/ZhaoHua.ttf:fonts/cjk.ttf"]   # CJK 字体部署（源:目标）
"""


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
  // WebGL 上下文丢失探针：浏览器回收 GL 上下文（资源压力）时触发——
  // 画布变黑而 RAF 仍活的"跑几分钟黑屏"现象的直接判据
  const cv = document.getElementById('starfish-we');
  cv.addEventListener('webglcontextlost', e => {{
    e.preventDefault();
    el.textContent = 'CTX-LOST @raf ' + n;
    el.style.color = '#f66';
    _wl.textContent += '[CTX-LOST] WebGL 上下文丢失 @raf ' + n + '\\n';
    _wl.scrollTop = _wl.scrollHeight;
  }}, false);
  cv.addEventListener('webglcontextrestored', () => {{
    el.style.color = '#6c6';
    _wl.textContent += '[CTX-RESTORED] 上下文恢复（引擎侧无自动重建——须刷新）\\n';
  }}, false);
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


sys.path.insert(0, str(Path(__file__).resolve().parent))
from pack_common import load_cfg


def load_toml(path: Path) -> dict:
    return load_cfg(path)


def build_one(name, build_ts):
    toml = PACK_DIR / f'{name}.web.toml'
    if not toml.is_file():
        sys.exit(f'[FAIL] 未找到 {toml}（先 --init {name}）')
    cfg = load_toml(toml)
    app = cfg.get('app', {})
    windowed = app.get('windowed', True)
    caption = app.get('caption', name)
    feats = cfg.get('build', {}).get('features', [])
    kind = 'canvas' if windowed else 'console'
    print(f"=== 构建 {name}（{kind}）===")
    feat_args = ['--features', ','.join(feats)] if feats else []
    sh(['cargo', 'build', '--release', '--target', 'wasm32-unknown-unknown',
        '--example', name, *feat_args])
    wasm = TARGET_DIR / f'{name}.wasm'
    if not wasm.is_file():
        sys.exit(f'[FAIL] wasm 产物缺失: {wasm}')
    sh(['wasm-bindgen', '--out-dir', str(WEB), '--target', 'web', str(wasm)])

    tpl = CANVAS_TEMPLATE if windowed else CONSOLE_TEMPLATE
    (WEB / f'{name}.html').write_text(
        tpl.format(name=name, caption=caption, build_ts=build_ts), encoding='utf-8')
    print(f"[OK] web/{name}.html + 胶水 + wasm\n")


def main():
    ap = argparse.ArgumentParser(description='pygame-rs Web 构建（toml 驱动）')
    ap.add_argument('names', nargs='*', help='案例名（pack/<名>.web.toml）')
    ap.add_argument('--all', action='store_true', help='构建 pack/*.web.toml 全部')
    ap.add_argument('--init', metavar='名', help='生成 pack/<名>.web.toml 模板')
    args = ap.parse_args()

    if args.init:
        PACK_DIR.mkdir(exist_ok=True)
        t = PACK_DIR / f'{args.init}.web.toml'
        if t.exists():
            sys.exit(f'[FAIL] 已存在: {t}')
        t.write_text(WEB_TEMPLATE.format(name=args.init), encoding='utf-8')
        print(f'[OK] 模板已生成: {t}（改完直接 python build_web.py {args.init}）')
        return

    if args.all:
        names = [p.name.removesuffix('.web.toml')
                 for p in sorted(PACK_DIR.glob('*.web.toml'))]
    else:
        names = args.names
    if not names:
        ap.error('未指定案例名（或用 --all 打包 pack/*.web.toml 全部）')

    WEB.mkdir(parents=True, exist_ok=True)
    # CJK 字体部署（存在即跳过；全案例共享一份）
    fonts_dir = WEB / 'fonts'
    fonts_dir.mkdir(exist_ok=True)
    cjk_dst = fonts_dir / 'cjk.ttf'
    if not cjk_dst.is_file():
        if CJK_FONT_SRC.is_file():
            shutil.copy2(CJK_FONT_SRC, cjk_dst)
            print(f'[OK] CJK 字体部署: web/fonts/cjk.ttf')
        else:
            print('[WARN] CJK 字体源缺失（resources/ZhaoHua.ttf）——Web 中文将缺字')
    check_bindgen_version()
    from datetime import datetime
    build_ts = datetime.now().strftime('%Y%m%d%H%M%S')
    for n in names:
        build_one(n, build_ts)

    print('=== 测试方式 ===')
    print(f'cd {WEB}')
    print('python -m http.server 8000')
    print('浏览器打开 http://localhost:8000/<名>.html（file:// 无效）')


if __name__ == '__main__':
    main()
