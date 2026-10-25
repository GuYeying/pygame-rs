#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""桌面 Windows 打包脚本（产品化批次，2026-10-25）。

形态 = "python.exe" 通用运行器：**一个 pygame-runner.exe 跑任意游戏**
（main.py + 资源目录），而非每游戏编一个 example。产物 = dist/<名>/
（runner exe + main.py + 资源）+ 同名 zip。

配置 = windows.toml（每游戏一份，`--init <名>` 生成模板；buildozer
风格分节、更细粒度——本平台认识的键本平台用）：

    [app]
    name = "my_game"              # 案例名（examples/<name>.py 为源）
    title = "My Game"

    [build]
    mode = "release"              # RustPython 铁律：恒 release
    features = []                 # cargo features（gui/physics3d…）

    [assets]
    include = []                  # 额外资源（glob，相对仓库根）：
                                  #   ["resources/**"] → 拷贝保持相对路径
    fonts = ["resources/ZhaoHua.ttf:fonts/cjk.ttf"]  # 源:目标（字体预载）

用法：
    python build_windows.py --init my_game     # 生成 pack/my_game.windows.toml
    python build_windows.py my_game            # 读 toml 打包 → dist/my_game/
    python build_windows.py --all              # pack/*.windows.toml 全打
"""

import argparse
import glob
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from pack_common import load_cfg, ensure_toml

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

SCRIPT_DIR = Path(__file__).resolve().parent
CRATE_DIR = SCRIPT_DIR.parent
PACK_DIR = CRATE_DIR / "pack"
DIST_DIR = CRATE_DIR / "dist"

TEMPLATE = """\
# 桌面 Windows 打包配置（build_windows.py --init 生成；可直接改）
[app]
name = "{name}"                # 案例名（examples/<name>.py 为游戏源码）
entry = "main.py"             # 入口文件（dist 内文件名；内须 def main()）
title = "{name}"               # 分发名（dist 目录与 zip 名）

[build]
mode = "release"               # RustPython 铁律：恒 release（勿改）
features = []                  # cargo features：gui（默认含）/
                               #   physics3d / physics2d 按需追加

[assets]
include = []                   # 额外资源 glob（相对仓库根），如：
                               #   ["resources/**", "assets/**/*.png"]
fonts = ["resources/ZhaoHua.ttf:fonts/cjk.ttf"]   # 源:目标（CJK 预载）
"""


def die(msg: str) -> None:
    print(f"[FAIL] {msg}")
    sys.exit(1)


def sh(cmd: list, cwd: Path = None) -> str:
    print("+", " ".join(str(c) for c in cmd))
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, errors="replace")
    if r.returncode != 0:
        print(r.stdout[-3000:])
        print(r.stderr[-3000:])
        die(f"命令失败: {cmd[0]} …")
    return (r.stdout + r.stderr).strip()


def load_toml(path: Path) -> dict:
    return load_cfg(path)


def copy_pairs(entries: list, dst_root: Path) -> None:
    """fonts 形态（"源:目标"）：单文件定向拷贝（缺源静默跳过——可选资源）。"""
    for e in entries:
        src, _, dst = e.partition(":")
        src, dst = CRATE_DIR / src, dst_root / (dst or Path(src).name)
        if src.is_file():
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dst)
        else:
            print(f"[skip] 缺源: {src}")


def copy_globs(patterns: list, dst_root: Path) -> None:
    """include 形态（glob）：保持相对仓库根的目录结构拷贝。"""
    for pat in patterns:
        for p in glob.glob(str(CRATE_DIR / pat), recursive=True):
            p = Path(p)
            if p.is_file():
                rel = p.relative_to(CRATE_DIR)
                dst = dst_root / rel
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(p, dst)


def build_one(toml_path: Path, main_py: Path) -> None:
    cfg = load_toml(toml_path)
    name = cfg["app"]["name"]
    title = cfg.get("app", {}).get("title", name)
    features = cfg.get("build", {}).get("features", [])
    assets = cfg.get("assets", {})
    entry = cfg.get("app", {}).get("entry") or "main.py"

    src_py = main_py
    if not src_py.is_file():
        die(f"游戏源码缺失: {src_py}")

    print(f"=== [{title}] 构建 runner（release）===")
    feat_args = []
    if features:
        feat_args = ["--features", ",".join(features)]
    sh(["cargo", "build", "--release", "--bin", "pygame-runner", *feat_args], cwd=CRATE_DIR)
    runner = CRATE_DIR / "target" / "release" / "pygame-runner.exe"
    if not runner.is_file():
        die(f"runner 产物缺失: {runner}")

    print("=== 组装 dist ===")
    out = DIST_DIR / title
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    shutil.copy2(runner, out / "pygame-runner.exe")
    shutil.copy2(src_py, out / entry)
    # dist 运行清单：runner 按此加载入口（entry 语义的运行侧落点）
    manifest = f"[app]\nname = {name!r}\nentry = {entry!r}\n"
    (out / "game.toml").write_text(manifest, encoding="utf-8")
    copy_pairs(assets.get("fonts", []), out)
    copy_globs(assets.get("include", []), out)

    zip_path = DIST_DIR / f"{title}.zip"
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as z:
        for p in out.rglob("*"):
            if p.is_file():
                z.write(p, p.relative_to(out.parent))
    print(f"[OK] {out}  +  {zip_path}  ({zip_path.stat().st_size // 1024 // 1024} MB)")


def main() -> None:
    ap = argparse.ArgumentParser(description="pygame-rs 桌面打包（python.exe 形态；入口自动发现——免 --init）")
    ap.add_argument("targets", nargs="*", help="游戏名 / 路径（目录含 main.py 或 .py 文件）；无参 = games/ 全部")
    ap.add_argument("--init", metavar="名", help="仅生成 pack/<名>.windows.toml 模板（一般无需——首次打包自动落盘）")
    args = ap.parse_args()

    if args.init:
        t = ensure_toml(CRATE_DIR, args.init, "windows", TEMPLATE)
        print(f"[OK] 模板: {t}")
        return

    # 目标解析：无参 = games/ 全部；参数 = 名/路径自动发现
    from pack_common import resolve_game, scan_games
    games = []
    if not args.targets:
        games = scan_games(CRATE_DIR)
        if not games:
            die(f"{CRATE_DIR / 'games'} 下无游戏（每游戏一子目录含 main.py）")
    else:
        for a in args.targets:
            got = resolve_game(CRATE_DIR, a)
            if got is None:
                die(f"未找到游戏: {a}（games/<名>/main.py 或 examples/<名>.py）")
            games.append(got)

    for name, main_py in games:
        t = ensure_toml(CRATE_DIR, name, "windows", TEMPLATE)
        build_one(t, main_py)


if __name__ == "__main__":
    main()
