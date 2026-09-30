# 安卓 APK 打包 lib_name 坑与真机诊断手册

> 2026-09-28。window_test 真机"闪退"问题自 09-27 批次十六起存在，本日定位
> 并修复。本文沉淀根因、错误假设清单（防止重走弯路）、正确打包链与诊断
> 工具箱。接手 pygame-rs 安卓问题先读本文。

## 一、核心根因：`android.app.lib_name` 与 .so 不匹配（致命）

### 症状

APK 装上卓易通后**秒退**（200ms~2s 不等），无任何应用日志（进程死在
`run()` 第一行之前），hilog 仅见 `ANCO CREATE → DIED`。

### 根因

`android/AndroidManifest.xml` 模板中 NativeActivity 的元数据：

```xml
<meta-data android:name="android.app.lib_name"
           android:value="__LIB_NAME__" />
```

NativeActivity 启动时按 `lib_name` **精确 dlopen `lib<名字>.so`**。手动
"替换 APK 内 .so" 式重打包时只换了 `lib/arm64-v8a/*.so`，**manifest 里的
lib_name 仍是旧示例名**（如 rp_main_loop_android）→ 找不到库 →
`IllegalArgumentException: Unable to find native library xxx` 秒崩。
**应用自身代码一行都没执行过**——所有日志缺失、睡眠标记失效、阶段色不亮，
全部是这个 crash 的表症噪音。

### 模拟器一锤定音的报错（卓易通上看不到，模拟器 logcat 可见）

```
java.lang.IllegalArgumentException: Unable to find native library rp_main_loop_android
    at android.app.NativeActivity.onCreate(NativeActivity.java:164)
```

### 检查方法

```bash
aapt2 dump xmltree --file AndroidManifest.xml <apk> | grep -A2 lib_name
# value 必须 = 本示例的 [[example]] 名（window_test_android 等）
```

## 二、正确打包链（2026-09-29 起统一为脚本，本节保留手动步骤作原理说明）

**首选**：`python pygame-rs/scripts/build_apk.py <示例名> [--dir <crate目录>]`
（pygame-rs 自持、全 crate 共用、lib_name 自检内置——批次二十一）。
以下手动步骤即该脚本的内部逻辑（理解原理 / 脚本不可用时兜底）：

```bash
# 1) 编译 + strip
cd pygame-rs
export ANDROID_NDK_ROOT=D:/SDK/Android/ndk/30.0.16248370
cargo ndk -t arm64-v8a -P 26 -o jniLibs -- build --release --example <名>_android
$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-strip \
    jniLibs/arm64-v8a/lib<名>_android.so

# 2) 模板占位符替换（LIB_NAME 必须=示例名；ORIENTATION 用 landscape，
#    与历史可跑 APK 烘焙值一致）
sed -e 's/__LIB_NAME__/<名>_android/g' -e 's/__ORIENTATION__/landscape/g' \
    android/AndroidManifest.xml > target/apk_build/AndroidManifest.xml

# 3) aapt2 link 生成 base（manifest + resources.arsc）
aapt2 link --manifest target/apk_build/AndroidManifest.xml \
    -I $ANDROID_HOME/platforms/android-37/android.jar -o target/apk_build/base.apk

# 4) 组装：base + classes.dex（robius 胶水，取自任意历史 APK）+ .so
#    （.so 必须 ZIP_DEFLATED 压缩存储，见 §三）→ zipalign 4 → apksigner
```

python 组装要点：`zout.writestr(item, data)` 透传原 ZipInfo 保持各条目
压缩方式；新写 .so 显式 `compress_type=zipfile.ZIP_DEFLATED`；
`resources.arsc` 恒 stored。最后 zipalign → apksigner（debug.keystore）。

## 三、排查中证伪的假设（勿重走）

| 假设 | 结论 |
|---|---|
| RustPython 链接导致入口崩溃 | ✗ 证伪：pygame_hello（无 RP）与同宏同路径全通；RP 相关挂起是**另一个独立问题**（rp_main_loop 黑屏挂起，见 §六） |
| .so 未压缩（ZIP_STORED）导致闪退 | ✗ 主要证伪：55ms vs 273ms 的差异只是 lib_name crash 的安装器路径噪音。但**保持 DEFLATED 约定**（与历史可跑 APK 一致，避免未知变量） |
| /sdcard 分区存储不可写 | ✗ 未证 Wimbledon：早期 rp 版本（写文件版）确实写过 rp_log.txt。卓易通对 /sdcard/Download 的直写可用性**随安装状态变化**，日志通道不能只依赖它 |
| 模拟器 GLES 死穴波及真机 | ✗ 真机 GLES 正常（pygame_hello 完美渲染）。死穴只影响模拟器渲染验收 |
| manifest 包名不匹配日志路径 | ✗ 包名就是 com.starfish.test，路径本身没错 |

## 四、真机（卓易通）诊断工具箱（按可信度排序）

1. **hilog 进程寿命**（最可信）：`ANCO CREATE/DIED pid=...` 两行时间差 =
   存活时长。配合**睡眠标记法**（在各阶段 `thread::sleep(6s)`，寿命读数
   =到达阶段）可在零日志条件下二分定位。
2. **模拟器 + adb logcat**（一锤定音）：逻辑层问题（尤其 pre-run 死亡）
   上 x86_64 包进模拟器，`adb logcat -b crash` 直接给 Java 层异常——本次
   lib_name 根因即由此取得。GLES 死穴只挡渲染验收，不挡逻辑定位。
   `adb shell am start -n com.starfish.test/android.app.NativeActivity`。
3. **屏幕阶段色**：各步骤 `fill` 不同颜色，快照/肉眼读数。注意：set_mode
   完成前无法上屏，只能覆盖其后的阶段。
4. **TCP 直传 PC**：应用内 `TcpStream` 单行 POST → PC 收集器
   （devlog_server.py）。**要求手机与 PC 同网段**（路由器 AP 隔离会断，
   宿主 ping 通 ≠ 卓易通容器通）。换同网段 Wi-Fi 后实测可用。
5. **hdc file send/pull**：`MSYS_NO_PATHCONV=1` 必加（git-bash 路径改写）。
   卓易通沙盒（/data/app/el2/...）shell 不可读，应用日志只能走应用自写
   /sdcard 或 TCP。
6. **快照**：`hdc shell snapshot_display -f /data/local/tmp/x.jpeg` +
   `hdc file recv`。连拍监视 hilog 可抓闪退瞬间。

## 五、启动时序坑：延迟 set_mode × 横屏旋转（本日第二个坑）

诊断用睡眠把 set_mode 推迟到启动后 6 秒 → 必死：manifest 烘焙
`screenOrientation=landscape`，启动早期发生**横屏旋转的
TerminateWindow/InitWindow**（ANativeWindow 销毁重建），迟到的 set_mode
拿着旋转前的旧 window 配 wgpu surface → `Invalid surface` → Err 退出
（真机 9.9s：6s 睡眠 + ~4s GPU 装配；模拟器同款报错见 GLES 死穴）。

**定约：启动后立即 set_mode**（pygame_hello/hello 形态），旋转交给帧循环
的 `Resized → surface.resize()` 消化。启动路径上不要有睡眠/长阻塞。

## 六、遗留问题（独立批次）

- **rp_main_loop 真机黑屏挂起（已随示例退役关闭，2026-09-29 批次 E）**：
  set_mode 调用缺失（批次十七日志与现文件不符，疑编辑丢失）→ 首次
  `fill()` 的 `get_screen()` 应 panic，实际却挂起——挂点在 RP genesis
  附近。示例已停用（存档 `examples/legacy/`，形态由 binding_probe 取代），
  不再排查；复活时优先补 set_mode。
- **xtask `--dir` 兄弟 crate 支持**：做完后 §二手动链可退役。
- **android-activity 0.6.1 胶水雷**：进程缓存复用 + 急速重启（<2s）会触发
  "销毁中途重建" abort（本轮 2.8s 死亡即此）——与 starfish-window CLAUDE.md
  记载同源，用户侧表现为"马上重开就闪退，等一会儿就好"。

## 七、本轮关键文件

| 项 | 路径 |
|---|---|
| 修正后的打包产物 | `target/android-apk/window_test_android.apk`（aapt2 链） |
| base 构建目录 | `target/apk_build/`（替换后的 manifest + base.apk） |
| TCP 日志收集器 | `devlog_server.py`（PC :9000，落 devlog.txt） |
| 诊断对照 APK | `target/android-apk/pygame_hello_android.apk`（RP-free 对照组） |
| hilog 捕获 | `wt_hilog_capture.log` / `wt_hilog2.log`（临时，可删） |

---

## 八、诊断通道自身的坑（批次十九补，2026-09-28 晚）

诊断手段自己也会坏，本批三连坑（全部实测）：

1. **TCP 无超时阻塞 connect = ANR**：`TcpStream::connect` 网络不通时在
   主线程无限期阻塞——表面"应用无响应"。**定约：诊断 TCP 必须
   `connect_timeout(300ms)` + 一次失败即拉黑**（AtomicBool，不再重试）。
2. **manifest 缺存储权限 → /sdcard 写入 EACCES**：`/sdcard/Download/日志`
   静默失败的全部根因（dlog 的 `if let Ok` 还会把失败吞掉）。模板已补
   `WRITE/READ_EXTERNAL_STORAGE` + `<application
   android:requestLegacyExternalStorage="true">`（卓易通按 legacy 授予，
   实证可写）。**改 manifest 后必须重造 base.apk**（aapt2 link）。
3. **日志通道不可信时的替代序**：hilog `ANCO CREATE/DIED` 寿命（唯一
   全时可用）→ 模拟器 logcat（逻辑层）→ 屏幕快照（`snapshot_display`）
   → TCP（需同网段）→ 文件（需权限）。

## 九、RP 原生函数绑定坑（批次十九实测；批次 A（2026-09-29）签名整形后修订）

> **批次 A 起 draw 面（`sf_draw_*`/`sf_draw_fill`）已 `#[derive(FromArgs)]`
> 结构体收参**（`args.rs` 的 `SfColor/SfRect/SfPoint/SfPoints` 承担
> TryFromObject 整形）——kwargs、缺省值、3/4 元颜色、嵌套点列全部原生支持，
> 下列 1~3 条**仅适用于旧扁平别名与未整形函数**（`vm.new_function` 直注册
> 形态）。签名契约回归：`cargo run --release --example sig_probe`。

1. **7 元组上限**：`new_function` 的 `Fn` impl 宏只展开 0..=7 个前置参数
   （builtin.rs 宏重复）。**FromArgs 结构体收参可绕开**（结构体算 1 参）；
   颜色等多值参数在旧形态下收敛为单 tuple 形参；
2. **tuple FromArgs = 平铺消费连续位置参数**（不是嵌套 tuple！）：
   `color: (i32,i32,i32,i32)` 形参要在 Python 侧 `*rgba(color)` 散开传
   ——传单个 tuple 会报 `Expected type 'int' but 'tuple' found`。
   批次 A 后用自定义 TryFromObject 类型（SfColor）替代；
3. **Rust tuple / `Vec<(i32,i32)>` 无 TryFromObject 实现**：点列旧方案是
   扁平化 `[x,y,x,y,...]` 传 `Vec<i32>`；批次 A 后 `SfPoints` 直接收
   `[(x, y), ...]` 嵌套形态；
4. **`types.ModuleType` 常量须 `setattr` 到模块对象**：shim 里的裸
   global 对 `pygame.QUIT` 不可见——常量定义循环里同时 `globals()[_name]`
   与 `setattr(pygame, _name, _val)`；
5. 生成器脚本**不要**模块级自调用（`game = game()`）而壳里再 `call`——
   二次调用报 `'generator' object is not callable`。定约：脚本只定义，
   壳负责实例化；
6. **RP debug 构建（Windows 桌面）引导 RecursionError（已修，2026-09-29）**：
   根因不是 `recursion_limit`（那是 Python 帧计数守卫，init_hook 抬 1000
   管不到这里），而是 `with_recursion` 的 **C 栈软限位**——
   `check_c_stack_overflow` 量真实栈指针（`GetCurrentThreadStackLimits`），
   debug margin 256KB（`STACK_MARGIN_BYTES`），而 Windows 主线程栈 Reserve
   默认仅 1MB（PE 头 /STACK）→ 冻结 importlib 引导的 debug 原生帧深即撞线
   （`essential initialization failed`；release margin 仅 64KB 故无恙）。
   修复 = `build.rs` 对 windows-msvc 示例
   `rustc-link-arg-examples=/STACK:16777216`（Reserve ≠ Commit，不占内存，
   CPython 的 python.exe 同款做法）。**debug 示例现已可用**，性能验收仍以
   release 为准。
7. **原生 pygame 模块注入的时点（批次 B 实测）**：`install` 若放 init_hook
   ——解释器初始化深处（importlib 引导未完）动模块机制 → **无限递归爆栈**
   （连 `import sys` 都炸，STATUS_STACK_OVERFLOW）。定约：宿主壳在
   `interp.enter` 内、essential init 完成后调 `rpy::install_pygame(vm)`。
   pymodule 宏备忘（内部标记 / 子模块嵌套不带 sub / with 语义）见
   `native_pygame.rs` 头注释与 CLAUDE.md。
8. **`Event.key` 自批次 C 起 typed**：`keydown`/`keyup` 事件的 `key` 位
   是 `pygame.Key` 实例（Comparable 值相等），不再是 int——脚本
   `e.key == pygame.K_x` 写法不变成立；K_* 全集（108 键）注册在 pygame
   顶层（`pygame.K_a` 等，typed）。整数键码不存在（引擎无 pygame 键码
   表，typed 是 starfish locals 既定决策）。

## 十、已知未结：首发图元缺失（偶发）与 Resized 接线缺失

- **症状**：某版 binding_probe 首启只有清屏色、`with` 会话图元全缺，
  重装后消失（未复现）。
- **主嫌疑**：启动早期横屏旋转重建 ANativeWindow，而 pygame 层**无人
  响应 `Resized` 调 `Screen::resize()`**（桌面/Web 无旋转无感，Android
  是真实缺口）——正交相机/交换链不跟随。
- **保险**：bp_log.txt 诊断计数器常驻（session_begin/draw/session_end/
  frame #n），复发时直接留证。
- **修法**（下一批）：pygame 层事件泵里 `Resized { width, height }` →
  `screen.resize(width, height)`（对齐 pygame_hello 基座的处理方式）。
  顺带可消掉"急速重启胶水 abort"的部分表象。
