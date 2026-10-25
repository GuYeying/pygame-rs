#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""打包脚本共享工具（产品化批次）：toml 配置读取（零依赖 fallback）。

优先级：tomllib（3.11+）→ tomli（若装）→ 内置 mini 解析（覆盖打包
配置的语法子集：[section] / key = "字符串" / true|false / [] 数组 /
# 行注释与行内注释）。打包 toml 由 --init 模板生成、形态受控，
mini 解析够用且零依赖。
"""

from pathlib import Path


def load_cfg(path: Path) -> dict:
    try:
        import tomllib
        with open(path, "rb") as f:
            return tomllib.load(f)
    except ImportError:
        pass
    try:
        import tomli
        with open(path, "rb") as f:
            return tomli.load(f)
    except ImportError:
        pass
    return _mini_parse(path)


def _strip_comment(line: str) -> str:
    """剥 # 注释（引号内的 # 保留）。"""
    out, in_str = [], False
    for ch in line:
        if ch == '"':
            in_str = not in_str
        elif ch == "#" and not in_str:
            break
        out.append(ch)
    return "".join(out).strip()


def _parse_value(v: str):
    v = v.strip()
    if v.startswith("["):
        inner = v[v.index("[") + 1:v.rindex("]")]
        items = [i.strip().strip('"') for i in inner.split(",") if i.strip()]
        return items
    if v.startswith('"'):
        return v.strip('"')
    if v in ("true", "false"):
        return v == "true"
    try:
        return int(v)
    except ValueError:
        pass
    try:
        return float(v)
    except ValueError:
        return v


def _mini_parse(path: Path) -> dict:
    cfg, section = {}, None
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = _strip_comment(raw)
        if not line:
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1].strip()
            cfg.setdefault(section, {})
            continue
        if "=" in line and section is not None:
            k, _, v = line.partition("=")
            cfg[section][k.strip()] = _parse_value(v)
    return cfg


# ── 打包目标自动发现（产品化批次 58：入口简化——免 --init）──────────
#
# 定约：
# - 参数为**路径**：目录（含 main.py）→ 名=目录名；或 .py 文件 → 名=去后缀
# - 参数为**名字**：优先 games/<名>/main.py（用户游戏），次 examples/<名>.py
#   （存量案例——web/apk 编译单元在此）
# - **无参**：打包 games/ 下全部（约定目录，用户游戏的家）
# - toml 缺失：按平台模板**自动生成**到 pack/（!init 的手动步退役为
#   首次自动落盘；后续可改 toml 定制）

GAMES_DIR_NAME = "games"


def resolve_game(crate: Path, arg: str):
    """参数（名或路径）→ (name, main_py: Path)。找不到 = None。"""
    from pathlib import Path as _P
    p = _P(arg)
    # 1) 路径形态：.py 文件（绝对 / 相对仓库根）
    for f in (p, crate / p):
        if f.suffix == ".py" and f.is_file():
            return f.stem, f
    # 2) 目录形态：内含 main.py（路径 / games/<名> / examples/<名>）
    for d in (p, crate / p, crate / GAMES_DIR_NAME / arg,
              crate / "examples" / arg):
        if d.is_dir() and (d / "main.py").is_file():
            return d.name, d / "main.py"
    # 3) examples 直文件（<名>.py，存量案例形态）
    f = crate / "examples" / f"{arg}.py"
    if f.is_file():
        return arg, f
    return None


def scan_games(crate: Path):
    """无参模式：games/ 下全部（每子目录含 main.py 即一个游戏）。"""
    games = crate / GAMES_DIR_NAME
    if not games.is_dir():
        return []
    return [(d.name, d / "main.py") for d in sorted(games.iterdir())
            if d.is_dir() and (d / "main.py").is_file()]


def ensure_toml(crate: Path, name: str, platform: str, template: str) -> Path:
    """toml 存在则返回；缺失则按模板自动生成（首次自动落盘）。"""
    pack = crate / "pack"
    pack.mkdir(exist_ok=True)
    t = pack / f"{name}.{platform}.toml"
    if not t.is_file():
        t.write_text(template.format(name=name), encoding="utf-8")
        print(f"[cfg] 自动生成默认配置: {t}（可改后重打）")
    return t
