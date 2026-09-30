# pygame-rs

**starfish 游戏引擎的 Python 绑定层**（独立 crate）：内嵌
[RustPython](https://github.com/RustPython/RustPython) 解释器 + 原生
`pygame` 模块，让你用标准 pygame 心智在 starfish 引擎上写游戏。

- **三平台**：Windows 桌面 / Web（wasm32）/ Android（arm64 真机，鸿蒙 NEXT + 卓易通容器实测）
- **原生绑定面**：`import pygame` 直达 Rust 原生模块（`#[pymodule]` 树），零 Python shim
- **依赖单向**：`pygame-rs → starfish`（引擎本体零 Python 资产）

```python
import pygame

def game():
    screen = pygame.display.set_mode((800, 600))
    while True:
        with screen.render() as r:
            r.rect((0, 200, 90), (20, 20, 120, 80))
        pygame.display.flip()
        yield   # 生成器门：yield = 帧边界
```

---

## 目录

- [前置准备](#前置准备)
- [桌面运行](#桌面运行)
- [Web 构建](#web-构建)
- [Android 构建](#android-构建)
- [示例清单](#示例清单)
- [文档指引](#文档指引)

---

## 前置准备

```bash
# Rust 编译目标（Web + Android；桌面无需额外目标）
rustup target add wasm32-unknown-unknown
rustup target add aarch64-linux-android

# Web 胶水工具（版本必须与 Cargo.lock 严格一致，当前 0.2.127）
cargo install wasm-bindgen-cli --version 0.2.127

# Android 打包工具
cargo install cargo-ndk
```

Android 另需环境变量（未设时脚本按常见路径探测，如 `D:/SDK/Android`）：
`ANDROID_HOME` / `ANDROID_NDK_ROOT` / `ANDROID_JAR`（dialog 用，如
`...\platforms\android-37.0\android.jar`）。

---

## 桌面运行

```bash
cargo run --release -p pygame-rs --example <示例名>
```

`--release` 为验收口径（debug 已可用，性能验收以 release 为准）。

---

## Web 构建

```bash
# 构建（wasm32 + JS 胶水 + HTML 加载页，产物落本仓库 web/）
python scripts/build_web.py <示例名>

# 本地服务（file:// 无效，必须走 http）
cd web && python -m http.server 8000
# 浏览器打开 http://localhost:8000/<示例名>.html
```

- 窗口类示例（binding_probe / font_test / draw_test / transparent_test）为 canvas 页；
  `sig_probe` / `rp_hello` 为控制台页（结果在 **F12**）。
- `--all` 全量构建；新增示例须在 `scripts/build_web.py` 顶部 REGISTRY 登记。
- Web 边界：无文件系统（资源走宿主预加载 / `load_from_bytes`）；音频须首次手势解锁
  （`pygame.mixer.needs_gesture()`）。

---

## Android 构建

```bash
# 统一打包链（唯一入口；xtask 已退役）
python scripts/build_apk.py <示例名>          # pygame-rs 示例
python scripts/build_apk.py hello --dir ..    # starfish 根 crate 示例共用本脚本
```

产物：`target/android-apk/<名>_android.apk`。

```bash
# 安装运行（模拟器冒烟必须 x86_64 原生包：--abi x86_64，arm64 走翻译层不可信）
adb install -r target/android-apk/<名>_android.apk
adb logcat | grep -E "selftest|diag"     # 卓易通不可信时看 /sdcard/Download/bp_log.txt
```

> 关键坑：秒退先查 `lib_name`（`aapt2 dump xmltree --file AndroidManifest.xml <apk>
> | grep -A2 lib_name`，value 必须 = `<名>_android`）；transform 源表面必须保活
> （GL 纹理 ID 复用串内容）。详见 CLAUDE.md「Android 实机关键坑」。

---

## 示例清单

| 示例 | 说明 |
|---|---|
| `sig_probe` | 签名契约锚点（**无头**，先于真机跑；130+ 项断言，终端看 `[OK]`/`FAIL`） |
| `binding_probe` | M1 绑定面验收（标准 pygame：with 会话渲染 + font + mixer + 手柄/触摸查询） |
| `font_test` | pygame.font 专项（构造/度量/旋转缩放/半透明/CJK 链路） |
| `draw_test` | pygame.draw 专项（六形状 + 半透明 + gfxdraw 兼容混绘） |
| `transparent_test` | 透明窗口专项（桌宠形态：`TRANSPARENT \| DIRECTX` 逐像素透明，仅 Windows） |
| `rp_hello` | M0 嵌入冒烟（RP 创世纪 / 语言特性 / 冻结 stdlib） |

---

## 文档指引

| 文档 | 内容 |
|---|---|
| `CLAUDE.md` | 项目全景 + 架构 + 坑位（接手先读） |
| `reference/三端构建与运行指南.md` | 三端构建一站式详细入口 |
| `reference/pygame绑定层API设计稿.md` | 绑定工程唯一契约 |
| `reference/pygame兼容层模块架构.md` | 模块映射 + Tier 排期 |
| `reference/安卓APK打包lib_name坑与真机诊断手册.md` | 安卓问题先读 |
| `reference/wasm编译与运行指南.md` | wasm 编译详解 |
| `../starfish/doc/PROJECT_STATUS.md` | 引擎侧进度主线 |

**逐模块独立测试**：共享宿主壳 `rpy::testhost::run(script, tag, transparent)`，
每模块一个 `examples/<模块>_test.py`（`[模块-test]` 行逐项断言 + 样张上屏）。
已完成：font_test / draw_test / transparent_test。
