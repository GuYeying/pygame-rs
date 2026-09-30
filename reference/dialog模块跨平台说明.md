# dialog 模块跨平台说明

> 源码：`src/base/dialog.rs`（feature = `dialog`）
> 定位（用户定稿）：**只做文件打开与保存**，消息框不做；TLS 亦不做（net 明文）。
> API 全部 **统一异步**——平台差异压进 future 体内部，调用方只面对同一套 await 接口。

---

## 一、API 与类型清单

| API | 签名 | 说明 |
|---|---|---|
| `pick_file` | `async (title, filters) -> Result<Option<PickedFile>, DialogError>` | 打开单文件（None = 取消） |
| `save_bytes` | `async (file_name, data) -> Result<Option<PathBuf>, _>` | **数据驱动保存**（见 §三） |
| `PickedFile` | `name()` + `read()` | 平台无关文件抽象（§四） |
| `Filter` | `(&'static str, &'static [&'static str])` | (展示名, 扩展名们) |
| `DialogError` | `UnsupportedPlatform` / `Backend(String)` | 不可用平台显式报错 |

## 二、平台能力矩阵（打开 / 保存全平台 ✅）

| API | Windows | Linux(GTK3) | macOS | Web | Android | iOS |
|---|---|---|---|---|---|---|
| pick_file | rfd ✅ | rfd ✅ | rfd ✅ | input[file] 读内存 ✅ | robius DocumentPicker ✅ | robius UIDocumentPicker ✅ |
| save_bytes | 对话框+写盘 ✅ | 同左 | 同左 | Blob 下载 ✅ | robius 选位置写数据 ✅ | 同左 |

构建依赖：桌面 = rfd（Linux GTK3 需 `libgtk-3-dev`）；
移动端 = robius-file-picker（**Android 构建需 `ANDROID_JAR` 环境变量**）；Web = 零额外。

## 三、阻塞 / 非阻塞行为（统一异步的两类真实执行模型）

| 平台 | pick（打开） | save_bytes（保存） |
|---|---|---|
| 桌面 | rfd **阻塞体 + async 外壳**——await 处即阻塞处（模态语义） | 同左：选完路径写盘后返回 |
| Web | `<input type=file>` + FileReader——**真异步**：await 挂起，引擎帧循环继续跑 | Blob + `<a download>` 同步触发下载（await 立即返回） |
| 移动端 | robius 回调 → mpsc channel 收敛为阻塞（await 处阻塞至用户操作） | 同左：用户选位置写数据后返回 |

### 与引擎同步 `frame()` 的配合

`frame()` 是同步回调，不能直接 `.await`。三平台模式：

```rust
// 桌面：帧内阻塞（模态语义；网络正确性不受影响——net 独立线程）
let picked = pollster::block_on(dialog::pick_file(None, &[("文本", &["txt"])]))?;

// Web：spawn_local 承接（挂起不挡帧）；建议从用户交互事件里发起
// （Safari 等要求文件选择由用户手势触发）
#[cfg(target_arch = "wasm32")]
fn on_open_clicked() {
    wasm_bindgen_futures::spawn_local(async move {
        match dialog::pick_file(None, &[("文本", &["txt"])]).await {
            Ok(Some(file)) => { let bytes = file.read().unwrap(); /* … */ }
            Ok(None) => { /* 取消 */ }
            Err(e) => { /* 错误 */ }
        }
    });
}
```

## 四、PickedFile——平台无关的文件抽象

| 平台 | 内部持有 | `read()` |
|---|---|---|
| 桌面 | 路径 | 惰性读盘（`std::fs::read`） |
| Web | 全量字节（选择时经 FileReader 已入内存） | 返回内存副本 |
| 移动端 | 临时路径（`content://` 经 into_local_file 落地） | 读盘 |

调用方只依赖 `name()` + `read()`。

## 五、内部架构（维护者视角）

### 结构与类型

```
src/base/dialog.rs
├── 公开层：DialogError / Filter / PickedFile / 三个 async fn
└── imp 模块 ×3（cfg 三选一，同名私有 async fn 签名）
    ├── 桌面：rfd 直调（阻塞体 + async 外壳）
    ├── Web：input[file] + FileReader + oneshot + spawn_local
    └── 移动端：robius 回调 + mpsc 阻塞收敛
```

| 内部类型 | 说明 |
|---|---|
| `imp::message_impl / pick_file_impl / pick_files_impl / save_bytes_impl` | 三平台同形 async fn，公开 API 纯转发 |
| `PickedFile { name, [path \| data] }` | 桌面/移动持路径（read 惰性读盘）；Web 持全量字节（read 返回副本） |
| `oneshot`（仅 Web） | 手写单线程 future 通道（Rc<RefCell> + Waker，无 Send 约束）——衔接浏览器事件与外层 await |
| `mem::forget(input / on_change)`（Web） | 单发对话框的 input 与回调随调用泄漏——换取免去自引用清理 |

### Web 文件选择的异步链路

```
pick_file() 调用
  → 创建 <input type=file>（accept=过滤器）→ onclick() 弹出选择器
  → [用户操作] onchange 事件
  → spawn_local(async)：FileList 逐文件 arrayBuffer() 读取
  → oneshot 唤醒外层 await → PickedFile 交付
（input 元素与回调闭包 mem::forget；未选文件直接确认 = Ok(None) 取消）
```

### 移动端阻塞收敛

```
robius pick_file(callback) 立即返回
  → callback（robius 内部线程）→ to_picked(content:// 落临时路径) → tx.send
  → 主线程 rx.recv() 阻塞等结果 → 交付
```

## 六、已知边界（如实声明）

1. **Web pick 的手势要求**：Safari 等要求文件选择由用户手势触发——
   建议从点击/按键事件回调里发起调用，勿在帧循环中无条件弹出。
2. **Web 单发泄漏**：`input` 元素与 onchange 回调随调用 `mem::forget`
   （单发对话框量级可忽略；换取免去自引用清理的复杂度）。
3. **Web pick_file 的 `title` 被忽略**（浏览器选择器无自定义标题）。
5. **save_bytes 的平台返回差异**：桌面 `Some(路径)`；Web/移动 `Ok(None)`
   （成功但无路径概念）——需要路径反馈的桌面场景可用，跨平台代码以
   "已保存"语义消费即可。
6. **移动端构建**：Android 需 `ANDROID_JAR` 环境变量（robius 编译
   Java/Kotlin 胶水）；iOS darwin 目标编译已验证，真机随引擎引导。
