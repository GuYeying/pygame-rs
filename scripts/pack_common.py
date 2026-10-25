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
