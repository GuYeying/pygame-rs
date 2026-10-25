#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""starfish Android APK 统一打包脚本（xtask 退役后的唯一打包链）。

归属：pygame-rs 自持（批次二十一，2026-09-29），根 crate 示例共用
（--dir 指向仓库根即可）。链路 = 批次十八/十九真机验证过的五步：

  1. cargo ndk 交叉编译（release；RP 恒 release 铁律）
  2. llvm-strip
  3. manifest 模板替换（__LIB_NAME__=<示例>_android / __ORIENTATION__）
     + aapt2 link 重造 base.apk        ← lib_name 坑的根治（批次十八）
  4. 组装 base + classes.dex + .so（.so 恒 ZIP_DEFLATED）
  5. zipalign 4 + apksigner（debug.keystore）

用法：
  python build_apk.py <示例名> [--dir <crate目录>] [--abi arm64-v8a]
                      [--out <产物目录>] [--dex <classes.dex路径>]

  示例名用桌面名（binding_probe），Android 侧自动取 <名>_android
  （仓库双注册惯例：桌面 <名> + [[example]] <名>_android cdylib）。
  --dir 默认本脚本所属 crate（pygame-rs）；根 crate 示例传 --dir ..

环境：ANDROID_HOME / ANDROID_NDK_ROOT（未设时按常见路径探测）。
"""

import argparse
import glob
import os
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

# Windows 控制台 GBK 编码兜底（✓/中文输出不再 UnicodeEncodeError）
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

ABI_DEFAULT = "arm64-v8a"
SCRIPT_DIR = Path(__file__).resolve().parent
CRATE_DIR_DEFAULT = SCRIPT_DIR.parent          # pygame-rs/（独立后仍成立）


def die(msg: str) -> "None":
    print(f"[FAIL] {msg}")
    sys.exit(1)


def best_subdir(base: Path, prefix: str) -> Path:
    """取 base 下 prefix 开头的最高版本子目录（build-tools/platforms/ndk）。"""
    cands = [p for p in base.glob(f"{prefix}*") if p.is_dir()] if base.is_dir() else []
    if not cands:
        die(f"未找到 {base}/{prefix}*（请检查 SDK 安装）")
    return max(cands, key=lambda p: [int(x) if x.isdigit() else 0 for x in p.name.split(".")])


def locate_sdk() -> Path:
    env = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    if env and Path(env).is_dir():
        return Path(env)
    for c in ("D:/SDK/Android", "C:/Android/Sdk", os.path.expanduser("~/AppData/Local/Android/Sdk")):
        if Path(c).is_dir():
            return Path(c)
    die("未找到 Android SDK（设 ANDROID_HOME）")


def locate_ndk(sdk: Path) -> Path:
    env = os.environ.get("ANDROID_NDK_ROOT") or os.environ.get("ANDROID_NDK")
    if env and Path(env).is_dir():
        return Path(env)
    ndk = best_subdir(sdk / "ndk", "")
    return ndk


def sh(cmd: list, cwd: Path = None, tail: int = 0) -> str:
    print("+", " ".join(str(c) for c in cmd))
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, errors="replace")
    if r.returncode != 0:
        print(r.stdout[-3000:])
        print(r.stderr[-3000:])
        die(f"命令失败: {cmd[0]} …")
    out = (r.stdout + r.stderr).strip()
    if tail:
        print("\n".join(out.splitlines()[-tail:]))
    return out


def find_dex(crate: Path, triple: str, explicit: str) -> Path:
    """classes.dex（robius 胶水）：优先显式路径 → 本 crate 构建产物扫描 → 历史 APK 兜底。"""
    if explicit:
        p = Path(explicit)
        if p.is_file():
            return p
        die(f"--dex 路径不存在: {p}")
    # 构建产物扫描（robius-file-picker build.rs 产物；文件名随版本，按 .dex 兜底扫）
    for p in (crate / "target").glob(f"{triple}/release/build/robius*/out/**/*.dex"):
        return p
    # 历史 APK 兜底（classes.dex 是与示例无关的胶水，可复用）
    for apk in sorted((crate / "target" / "android-apk").glob("*.apk")):
        try:
            with zipfile.ZipFile(apk) as z:
                if "classes.dex" in z.namelist():
                    print(f"[dex] 兜底取自历史 APK: {apk.name}")
                    import tempfile
                    tmp = Path(tempfile.gettempdir()) / "starfish_classes.dex"
                    tmp.write_bytes(z.read("classes.dex"))
                    return tmp
        except zipfile.BadZipFile:
            continue
    die("未找到 classes.dex（先成功构建一次含 dialog 特性的示例，或用 --dex 指定）")


def main() -> None:
    ap = argparse.ArgumentParser(description="starfish Android APK 统一打包")
    ap.add_argument("example", help="桌面示例名（如 binding_probe；Android 侧自动取 <名>_android）")
    ap.add_argument("--dir", default=str(CRATE_DIR_DEFAULT), help="示例所属 crate 目录（默认 pygame-rs；根示例传 ..）")
    ap.add_argument("--abi", default=ABI_DEFAULT)
    ap.add_argument("--out", default=None, help="产物目录（默认 <仓库根>/target/android-apk）")
    ap.add_argument("--dex", default=None, help="classes.dex 显式路径（默认自动探测）")
    ap.add_argument("--orientation", default="landscape", help="屏幕方向（默认 landscape，与历史可跑包一致）")
    ap.add_argument("--features", default=None,
                    help="cargo features（如 physics3d；缺省 = crate default features）")
    args = ap.parse_args()

    crate = Path(args.dir).resolve()
    if not (crate / "Cargo.toml").is_file():
        die(f"crate 目录无 Cargo.toml: {crate}")
    android_example = f"{args.example}_android"
    # 模板搜索：crate 自有 android/ 优先（独立 crate 形态），其次父目录
    # （monorepo 形态，如 starfish 根示例共用）
    manifest_t = None
    for c in (crate, crate.parent):
        t = c / "android" / "AndroidManifest.xml"
        if t.is_file():
            manifest_t = t
            break
    if not manifest_t:
        die("未找到 android/AndroidManifest.xml 模板（crate 目录或其父目录）")

    sdk = locate_sdk()
    ndk = locate_ndk(sdk)
    bt = best_subdir(sdk / "build-tools", "")
    jar = best_subdir(sdk / "platforms", "android-") / "android.jar"
    triple = {"arm64-v8a": "aarch64-linux-android", "x86_64": "x86_64-linux-android",
              "armeabi-v7a": "armv7-linux-androideabi"}.get(args.abi)
    if not triple:
        die(f"未知 ABI: {args.abi}")
    ndk_bin = ndk / "toolchains" / "llvm" / "prebuilt" / "windows-x86_64" / "bin"

    out_dir = Path(args.out) if args.out else (crate / "target" / "android-apk")
    out_dir.mkdir(parents=True, exist_ok=True)
    jni_libs = crate / "jniLibs"
    so_name = f"lib{android_example}.so"

    # 1) 交叉编译（release 铁律：RP debug genesis 必败）
    print(f"=== 1/5 cargo ndk build ({args.abi}) ===")
    feat_args = ["--features", args.features] if args.features else []
    sh(["cargo", "ndk", "-t", args.abi, "-P", "26", "-o", str(jni_libs),
        "--", "build", "--release", "--example", android_example, *feat_args],
       cwd=crate, tail=1)
    so = jni_libs / args.abi / so_name
    if not so.is_file():
        die(f"编译产物缺失: {so}")
    # 2) strip（strip 后查符号必须 nm -D 看动态表）
    print("=== 2/5 llvm-strip ===")
    sh([str(ndk_bin / "llvm-strip"), str(so)])
    nm = sh([str(ndk_bin / "llvm-nm"), "-D", str(so)])
    if "T android_main" not in nm:
        die("android_main 未导出（NativeActivity 将无法启动）——检查 app_entry! 接线")
    print("[nm] android_main 已导出 ✓")

    # 3) manifest 模板替换 + aapt2 link（lib_name 根治点，批次十八）
    print("=== 3/5 aapt2 link（lib_name 对齐）===")
    work = crate / "target" / "apk_build"
    work.mkdir(parents=True, exist_ok=True)
    mtext = manifest_t.read_text(encoding="utf-8")
    mtext = mtext.replace("__LIB_NAME__", android_example).replace("__ORIENTATION__", args.orientation)
    m = work / "AndroidManifest.xml"
    m.write_text(mtext, encoding="utf-8")
    base_apk = work / "base.apk"
    # 主题资源（res/values：黑底 splash/windowBackground，批次 D 白闪修复）——
    # res 目录存在才编译；aapt2 link 以编译后 flat 传参
    res_dir = manifest_t.parent / "res"
    link_args = [str(bt / "aapt2.exe"), "link", "--manifest", str(m), "-I", str(jar), "-o", str(base_apk)]
    if res_dir.is_dir():
        res_zip = work / "res.zip"
        sh([str(bt / "aapt2.exe"), "compile", "--dir", str(res_dir), "-o", str(res_zip)])
        link_args.append(str(res_zip))
    sh(link_args)

    # 4) 组装（resources/dex/so；.so 恒 DEFLATED——与历史可跑包一致）
    print("=== 4/5 组装 ===")
    dex = find_dex(crate, triple, args.dex)
    final = out_dir / f"{android_example}.apk"
    tmp_full = work / "full.apk"
    with zipfile.ZipFile(base_apk) as zb, zipfile.ZipFile(tmp_full, "w") as zout:
        for item in zb.infolist():
            zout.writestr(item, zb.read(item.filename))
        zout.writestr("classes.dex", Path(dex).read_bytes(), compress_type=zipfile.ZIP_DEFLATED)
        zout.writestr(f"lib/{args.abi}/{so_name}", so.read_bytes(), compress_type=zipfile.ZIP_DEFLATED)

    # 5) zipalign + 签名 + 验证 + 自检
    print("=== 5/5 zipalign + apksigner ===")
    aligned = work / "aligned.apk"
    sh([str(bt / "zipalign.exe"), "-f", "4", str(tmp_full), str(aligned)])
    ks = Path.home() / ".android" / "debug.keystore"
    if not ks.is_file():
        die(f"未找到签名 keystore: {ks}")
    sh([str(bt / "apksigner.bat"), "sign", "--ks", str(ks), "--ks-pass", "pass:android",
        "--out", str(final), str(aligned)])
    sh([str(bt / "apksigner.bat"), "verify", str(final)])
    # 自检：lib_name 与示例名一致（批次十八 lib_name 坑的永久哨兵）
    badging = subprocess.run([str(bt / "aapt2.exe"), "dump", "xmltree", "--file",
                              "AndroidManifest.xml", str(final)],
                             capture_output=True, text=True, errors="replace")
    ok = f'"{android_example}"' in badging.stdout
    print(f"[自检] lib_name={android_example} {'✓' if ok else '✗ 不匹配！'}")
    if not ok:
        die("lib_name 自检失败——禁止分发")
    tmp_full.unlink(missing_ok=True)
    aligned.unlink(missing_ok=True)
    print(f"[OK] {final}  ({final.stat().st_size // 1024 // 1024} MB)")


if __name__ == "__main__":
    main()
