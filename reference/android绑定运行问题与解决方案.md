# Android 绑定层运行问题与解决方案（详解版）

> 2026-09-23,批次 22/22·续。pygame-rs 首版 APK 真机闪退的完整定位、
> 机理、平台差异分析与修复记录。两个问题:① TLS 键耗尽(启动即崩);
> ② GPU 表面就绪时序(启动后冻结)。关联:changelog 批次 22/22·续;
> `reference/vendor与crates切换指南.md` 例外登记;诊断命令见文末速查。

---

## 〇、背景与症状

pygame-rs 在 android 上落地运行入口(M4)后,首版 APK 真机点击图标
**立即闪退**——无窗口、无提示、无 ANR。而同一份代码在桌面
(Windows)完全正常、在 Web(wasm)也已跑通。

这指向一类问题:**平台运行时差异**——编译期一切正常
(`cargo check --target aarch64-linux-android` 通过),差异只在使用了
不同的操作系统机制之后才爆发。最终确认是两个独立问题的叠加:

| # | 问题 | 崩溃形态 | 死因层 |
|---|---|---|---|
| 1 | TLS 键耗尽 | 启动即中止(abort) | bionic libc 的 pthread key 池 × RustPython 的静态量 |
| 2 | GPU 表面就绪时序 | 启动后引擎冻结 | ANativeWindow 异步挂载 × 引擎 start 门即时装配 |

---

## 一、定位过程与手法(可复用)

1. **复现**:真机不可达时用模拟器。`cargo xtask android <示例>
   --abi x86_64 --build` 出 x86_64 包(注意 arm64 包模拟器跑不了),
   `emulator -avd <名> -no-window -no-audio -gpu swiftshader_indirect`
   冷启动。
2. **收割崩溃信息**:`adb logcat -s RustStdoutStderr` 收 Rust 侧
   panic 与日志(android-activity 胶水自动把 stdout/stderr 重定向到
   这个 tag);`adb logcat -d | grep -aE "DEBUG|libc"` 收 tombstone 帧
   (自带符号化,能直接看到 crate::函数级回溯)。
3. **精确到崩溃点**:第一条 tombstone 帧显示
   `LazyKey::lazy_init → abort`,后面的帧显示
   `rapidhash RANDOM_SEED → StaticCell::set → PyIndexError::init_
   builtin_type → ExceptionZoo::init → Context::init_genesis →
   engine_main`——崩溃点(随机种子键)与真正的根因(此前几百个类型
   静态已把池吃穿)不在同一处,这是本案例的关键迷惑点。
4. **对照实验定性归属**(本案例最有价值的一步):用**最小无 Python 的
   引擎探针 probe_gfx** 跑同一环境——它同样炸 `Invalid surface`,
   由此把问题二干净地归给引擎/模拟器 GL 层,避免在绑定层瞎修。
   问题一则是反证:探针不崩、pygame-rs 崩 → 差异量在 RustPython。

---

## 二、问题一:TLS 键耗尽

### 2.1 什么是 TLS,thread_local! 在各平台怎么实现

TLS(Thread-Local Storage,线程局部存储)= 每个线程拥有一份独立
副本的全局变量。Rust 的 `thread_local!` 宏在各平台依赖不同的底层
机制,这正是本次事故的根源:

| 平台 | 非析构静态 | 带析构静态 | 单个 thread_local! 的"键"成本 |
|---|---|---|---|
| **Windows(MSVC)** | 原生 `.tls` 段(PE TLS 模板,**全模块共享一个槽位**) | FlsAlloc 回调 | ≈0(非析构静态几乎免费) |
| **Linux(glibc)/ macOS** | 原生 ELF TLS(`__thread`/`__attribute__((tls_model))`,同样是模板制) | `__cxa_thread_atexit_impl` 注册,不占 pthread key | ≈0 |
| **Android(bionic)** | **pthread key 模拟** | **pthread key + dtor 注册** | **恒为 1 个 pthread key** |
| **wasm** | 线性内存偏移槽位 | 无线程析构概念 | ≈0(无 OS 参与) |

关键差异:**android 是 rust 主流目标中唯一"每个 thread_local! 静态
无论是否有析构都要占一个 pthread key"的平台**。原因:ELF TLS 在
bionic 中成熟得晚,rust std 对 android 目标采取保守的 pthread key
模拟路线(本案例的 tombstone 直接证实:`LazyKey::lazy_init` →
`pthread_key_create` 失败 → abort)。

而 pthread key 池是**全进程共享**的,上限 `PTHREAD_KEYS_MAX = 1024`
——注意是进程级,不是线程级:静态 A 和静态 B 各占一个键,与哪个
线程访问它们无关。

### 2.2 崩溃链条

RustPython 的 `static_cell::non_threading` 实现:
每个内建类型(异常类 ~50 个、内建类 ~150 个、各模块缓存若干)
通过 `static_cell!` 宏生成**一个 thread_local 静态**用于缓存类型
对象指针。在 android 上 = 每个类型一个 pthread key。

```text
进程启动
└─ ART/系统组件预先占用少量键
└─ android-activity 胶水线程启动 → android_main → run_source
   └─ Context::init_genesis(解释器创世纪,初始化全部内建类型)
      ├─ ExceptionZoo::init:逐个异常类 init_builtin_type
      │    └─ 每个类型:thread_local 静态首用 → pthread_key_create
      │       (此刻池已接近耗尽)
      └─ PyIndexError::init_builtin_type
         └─ 内部 rapidhash RANDOM_SEED 静态首用
            → pthread_key_create → EAGAIN(池满!)
            → "fatal runtime error: out of TLS keys, aborting"
```

**关键迷惑点**:崩在 rapidhash(随机种子),但 rapidhash 无辜——
它只是压垮池的最后稻草。真正的问题是非线程局部语义被硬塞进
线程局部机制,再乘上 RustPython 的静态数量。

### 2.3 为什么编译期没发现

`cargo check`/`cargo build` 只保证类型与链接正确,不运行任何初始化
代码。TLS 耗尽是纯运行期、纯目标平台行为——**平台编译矩阵全绿
不等于平台能跑,首次落地的平台必须实跑一次**。这条已写进平台
矩阵批次(21)的教训栏。

### 2.4 解决过程(试错记录,含失败的中间步骤)

| 尝试 | 结果 | 学到什么 |
|---|---|---|
| ① 撤掉 256MB 大栈线程,直接 android_main 线程跑 | 仍崩,崩点不变 | 崩溃与线程身份无关——池是进程级的,静态数量才是变量;但此步仍保留:修好了 JNI 线程亲和(`internal_data_path` 等 JNI 调用要求 JVM 已 attach 的线程),并与 app_entry 惯例对齐 |
| ② 尝试开启 rustpython-common 的 `threading` feature(让 StaticCell 走 parking_lot 全局锁) | **编译失败** | 全局锁版 StaticCell 要求 `T: Sync`,而 RP 对象是 `!Sync` 的 Rc 系——`threading` 变体本就不是给单线程解释器对象准备的 |
| ③ **最终方案**:vendor 局部修改——android 专用 non_threading 分支,StaticCell 改为全局 `UnsafeCell<OnceCell<T>>` + `unsafe impl Sync` | 通过 | 绕不开库内部实现时,做**语义等价的最小替换**并登记 |

③ 的形态:android 分支下,`static_cell!` 宏生成的静态从
`thread_local! { OnceCell<&T> }` 变为普通
`static INNER: StaticInner<T>`(UnsafeCell 包全局 OnceCell),
`unsafe impl Sync` 仅为让静态声明通过类型检查。

### 2.5 解决思路(为什么这样改是安全的)

**核心论证:找到一个"语义等价但实现不同"的替换点。**

- 原实现用 thread_local 的目的:缓存**每线程**的类型指针,避免
  `&'static T` 要求 `T: Sync`(RP 对象是 Rc 系,`!Sync`)。
- 我们的嵌入契约:**GIL 单线程模型 + 每进程一个解释器 + 恒在同一个
  线程驱动**(见绑定层定案文档)。在这个契约下,"每线程各有一份"
  与"全进程共享一份"**可观察行为完全相同**——因为根本只有一个线程、
  一份。unsafe Sync 的责任由契约兜底,契约写进了修改处的注释。
- 反面教材是尝试②:试图沿库预设的 `threading` 方向走,但那个方向
  的类型约束(Sync)与 RP 对象模型(!Sync)根本冲突——**方向错了,
  feature 开关再干净也走不通**。

**语义红线(实测踩过)**:桌面/网页**必须保留原 thread_local 实现**,
不能"顺手统一"。原实现里"每线程独立 genesis"的隔离语义是行为
测试集的依赖——全局化后,并行测试的第二个线程会复用第一个线程的
genesis 上下文,事件/类型状态互相污染,**11 个测试交叉失败 6 个**。
这是"平台专用分支"而不是"全局替换"的原因。

---

## 三、问题二:GPU 表面就绪时序

### 3.1 背景:android 的窗口表面是异步挂载的

三平台"引擎 start 门(第一次 about_to_wait)执行时,原生窗口是否
已可用"的答案完全不同:

| 平台 | start 门时的表面状态 | 装配策略 |
|---|---|---|
| 桌面(windows/linux/mac) | 原生窗口在事件循环开始前已同步创建 | start 门同步装配 ✓ |
| Web(wasm) | canvas 在 DOM 里,但 GPU adapter 请求是异步的 | 异步装配(spawn_local + await)✓ |
| **Android** | **ANativeWindow 由系统 SurfaceCreated 异步挂载,start 门时通常还没挂上** | start 门装配 = 必撞 `Invalid surface` ✗ |

### 3.2 症状与机理

TLS 修复后,三锚点(init/set_mode/run 启动)全部出现,随后:

```text
wgpu error: Validation Error
  In Surface::configure
    Invalid surface
```

- `Invalid surface` = wgpu-core 把 GLES 后端 configure 返回的
  Outdated/Lost 类错误统一映射(见 wgpu-core device/resource.rs,
  对应"原生窗口指针此刻无效/不可用")。
- 更麻烦的二阶问题:装配走的 `RenderEntry::new` 内部是
  `pollster::block_on`,future 在 pollster 另起的线程上执行——panic
  发生在那个线程,**宿主的 `catch_unwind` 拦不住**,且 block_on 从此
  挂死:引擎冻结、进程存活、无任何后续帧。

### 3.3 解决:惰性装配 + 事件闸门

- `on_start` 在 android 上不装配(置 None);
- `AppCore` 增加 `resized_seen` 标记:**首个 `WindowEvent::Resized`
  置位**——winit android 只在 ANativeWindow 真正挂载后才派发
  Resized,这是平台自己给出的"表面就绪"信号,比"猜测尺寸非零就
  就绪"可靠(实测:尺寸非零时窗口仍可能无效);
- `on_frame` 每帧检查:闸门开 && GPU 槽空 && 帧数 % 15 == 0 →
  尝试装配;成功后走正常帧路径。

### 3.4 解决思路

1. **就绪是事件,不是时点**。凡是系统异步给的东西(ANativeWindow、
   WebGPU adapter、canvas 尺寸),一律"平台发就绪信号 → 才去拿",
   不做"start 时一定就绪"的假设。这条与 Web 端的异步装配是同一个
   思想,android 只是它的第二个应用场景,未来 iOS 同样适用。
2. **就绪信号用平台自己派发的,不要自己发明**(第一版用"窗口尺寸
   非零"猜就绪,失败——尺寸信息先于表面可用)。
3. **归属判定用对照实验**:probe_gfx(纯引擎、零 Python)在同一
   模拟器同样炸 `Invalid surface`、同一条 about_to_wait 路径——
   由此确认模拟器 SwiftShader GL + wgpu GLES 的表面问题属**引擎侧
   遗留**(真机 GPU/EGL 环境不同,惰性装配后实测通过),避免在
   绑定层修引擎的锅。

---

## 四、通用收获

1. **平台编译矩阵 ≠ 平台可运行**——TLS 这类纯运行期、纯平台行为
   的崩溃,check 全绿也会踩;新平台首次必须实跑。
2. **崩点 ≠ 根因**——崩溃栈最顶端的帧(rapidhash)只是最后一根
   稻草,往前找"谁把资源吃到临界点"。
3. **平台差异要下沉到机制层理解**——"thread_local 在 android 上
   的实现方式"决定了 RustPython 的静态量在哪个平台会出事;只记
   "android 有这个 bug"而不知道为什么,换个库还会再踩。
4. **修复优先找"语义等价的实现替换"**,替换的 soundness 论证必须
   落在明确写下的运行契约上;契约依赖什么(单线程?单解释器?),
   注释里写什么。
5. **平台专用分支 ≠ 可以顺手全局统一**——全局化是有测试背书的
   语义变更(并行测试依赖每线程隔离),红线要靠测试守着。

## 五、诊断命令速查

```bash
# 复现包(模拟器为 x86_64;arm64 包模拟器跑不了)
cargo xtask android pygame_hello --dir pygame-rs --abi x86_64 --build
# 装启收
adb install -r target/android-apk/pygame_hello_android.apk
adb logcat -c && adb shell am start -n com.starfish.test/android.app.NativeActivity
adb logcat -d -s RustStdoutStderr | tail -30          # Rust panic/锚点
adb logcat -d | grep -aE "DEBUG|libc" | tail -30      # tombstone 帧
adb shell pidof com.starfish.test                     # 进程存活判定
# 对照实验(归属定性:纯引擎、零 Python)
cargo xtask android probe_gfx --abi x86_64 --build
```

---

## 六、附:T9 深递归栈实验数据（2026-09-23,release 构建）

实验设计:`examples/stack_probe.rs` 在**显式大小**的线程栈上执行
`rec(n)=1+rec(n-1)` 深递归脚本,三参数可配(栈 MB / 递归深度 /
recursion_limit)。执行钩子 = `rpy::exec_probe`(当前线程同步执行,
recursion_limit 可配——它是 `VirtualMachine` 的运行期 Cell,非
Settings 字段,`sys.setrecursionlimit` 同源)。

### 数据(release;Windows x64 与 android x86_64 bionic 行为一致)

| 栈 | 递归深度 | 结果 |
|---|---|---|
| 2 MiB(android 胶水线程 = std 默认) | 5000 层(limit 10000) | **OK** |
| 2 MiB | 990 层(limit 1000) | **OK** |
| 16 MiB | 40000 层(limit 40100) | **OK** |
| 32 MiB | 99999 层(limit 100001) | **OK** |
| 2 MiB | 999(limit 1000,差一) | RecursionError(limit 语义:深度+基线帧 ≥ limit) |
| 64 MiB | 999999(limit 1000100) | RecursionError(native 余量守卫兜底,**无崩溃**) |

### 结论(数字)

- **每 Python 帧 native 栈成本 ≈ 400 B**(release;debug 构建帧更重,
  余量常量也相应翻倍:`STACK_MARGIN_BYTES` release 64 KiB / debug
  256 KiB)。
- **可用深度 ≈ (栈 − 64 KiB 余量) / 400 B**:2 MiB → ~5000 层;
  16 MiB → ~4 万层;32 MiB → ~8 万层;256 MiB → ~65 万层。
- **全矩阵零真爆栈**:RP 有 native 栈感知守卫(`psm` 读 SP vs
  栈底+软限),越线一律干净抛 `RecursionError`——与 CPython 语义
  一致,绝无段错误。
- **android 现状判定**:胶水线程 2 MiB + 默认 recursion_limit 1000
  (≈ 400 KB native)——安全余量约 5 倍,**无需调大**;用户脚本超
  1000 层 → `RecursionError`,与 CPython 同语义。
- **调大容量路径**(如需 >1000 层):绑定层入口自 spawn 引擎线程
  (`Builder::stack_size(16/32 MiB)`)——static_cell 全局化后 TLS 已
  线程无关,模拟器实证 16/32 MiB 在 bionic 上行为一致;待验证项 =
  新线程上的 JNI attach(`internal_data_path`),备选 = 装配前置到
  spawn 之前。android-activity 胶水线程本身无旋钮可调(裸
  `std::thread::spawn`)。

### 实验方法论提醒

- 深度 N 需要 `limit > N + 基线帧数`,差一会误判"栈不够"(首版
  实验踩过两次:999/1000 与 16 MiB/40000/40001 均为差一误读);
- 空消息异常必须报类型名(`err_str` 已改为"类名: 消息"格式)——
  裸 `RecursionError()` 的 str 是空串,只打消息会一无所知。
