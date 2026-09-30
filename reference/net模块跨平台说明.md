# net 模块跨平台说明

> 源码：`src/base/net.rs`（feature = `net`）
> 定位：游戏**客户端**网络（主动连接；监听/服务端待需求落地）。
> 核心决策：**网络 IO 独立于主线程与本地 IO**（后台线程/回调自驱），
> 主线程阻塞（弹窗/本地文件）不影响网络正确性——因此**零 tokio**。

---

## 一、API 与类型清单

| 项 | 类型/签名 |
|---|---|
| `TcpConn::connect(addr) -> Result<TcpConn, NetError>` | 发起连接（**立即返回**，后台握手 10s 超时） |
| `TcpConn::state() -> ConnState` | Connecting / Connected / Closed |
| `TcpConn::send(&[u8]) -> Result<(), NetError>` | 发一帧（消息语义） |
| `TcpConn::try_recv() -> Option<Vec<u8>>` | 收一帧（非阻塞；None = 暂无） |
| `TcpConn::close()` | 关闭 |
| `UdpSock::bind(local)` / `send_to(data, peer)` / `try_recv_from()` | UDP（非阻塞轮询；**Web 不支持**） |
| `Connection`（trait，pub） | 传输统一接口：`state / send / try_recv / close`——新传输（QUIC 等）的扩展点 |
| `NetError` | `UnsupportedPlatform` / `Backend(String)` |
| `ConnState` | Connecting / Connected / Closed |

**帧协议**：4 字节大端长度前缀 + payload（单帧上限 **16 MB**，收发双向校验）。
TCP 的流式字节经此自动分帧/重组——与 Web WebSocket 的消息语义对齐，
业务代码一套写法跨平台。

**地址格式**：`"host:port"`（原生）；Web 额外接受 `ws://` / `wss://` 前缀
（裸 `host:port` 自动归一为 `ws://`；要 TLS 用 `wss://host:port`）。

## 二、平台能力矩阵

| 能力 | Windows | Linux | macOS | Android/iOS | Web |
|---|---|---|---|---|---|
| TCP 消息连接 | ✅ 后台线程 | ✅ 后台线程 | ✅ 后台线程 | ✅ 后台线程（Android 需 INTERNET 权限） | ✅ WebSocket |
| UDP 轮询 | ✅ 非阻塞 socket | ✅ | ✅ | ✅ | ❌ `UnsupportedPlatform`（**创建即报**，无静默替代） |
| TLS | ❌ **定稿不支持**（明文） | 同左 | 同左 | 同左 | `wss://` 即 TLS ✅ |

## 三、内部架构与数据流

### 类型清单

| 类型 | 层 | 说明 |
|---|---|---|
| `Connection`（trait，pub） | 统一接口 | `state / send / try_recv / close`——传输扩展点 |
| `TcpConn`（pub） | 统一句柄 | `Box<dyn Connection>`；帧上限校验与状态查询的对外门面 |
| `NetError` / `ConnState` | 统一类型 | 错误 / 连接状态 |
| `NativeTcp`（cfg 非 wasm） | 传输实现 | state 原子 + 出站通道 + 入站通道 + 线程句柄 |
| `WebTcp`（cfg wasm） | 传输实现 | state 原子 + WebSocket 对象 + 入站队列 + 积压队列 + 回调组 |
| `UdpSock`（cfg 非 wasm） | 传输实现 | 非阻塞 socket 直轮询（**无线程、无队列**） |

### 原生线程模型（每 TCP 连接两线程）

```
主线程 (frame)                              网络线程 (run)            写线程 (tx)
    │ send(frame) ── to_net 通道 ─────────────────────────────▶│ recv → write_all(len+payload)
    │ try_recv() ◀─ from_net 通道 ◀── read_exact(len)→payload │
    └ close() → to_net(Close) ────────────────────────────────▶│ 退出
                 state 原子（Arc<AtomicU8>）：网络线程权威写 / 主线程读
```

- 出站通道（to_net）：Close 消息兼作写线程的退出信号
- 入站通道（from_net）：无界——不消费会积压（见 §六.3）

### Web 回调模型（单线程任务队列）

```
new WebSocket(url) ── onopen：state=Connected + 冲刷积压队列
send(frame) ────── Connected → ws.send(Uint8Array)
                   Connecting → pending 队列（≤512 帧，onopen 冲刷）
onmessage ──────── ArrayBuffer → inbound 队列 ◀── try_recv() 弹出
onclose / onerror ─ state=Closed
```

## 四、阻塞 / 非阻塞行为（核心语义）

### 原生（win/linux/mac/android/ios）：后台线程自驱

- `connect(addr)` 立即返回 `TcpConn`（Connecting 态）；后台线程执行
  解析 + 10s 超时握手 + 读写循环
- `send()`：帧写入出站通道（无界通道 + 连接断开检测；**永不阻塞主线程**）；
  **Connecting 期间自动入队**（上限 512 帧，满则报错），Connected 后按序冲刷
- `try_recv()`：非阻塞取一帧；后台读线程持续收帧入队
- **应用无需维护任何运行循环**——线程自驱；推荐每帧 `try_recv` 消费
  （见 §五 入站积压说明）

### Web（WebSocket）：浏览器任务队列自驱

- `new WebSocket(url)` + `binaryType=arraybuffer`（自持 js_sys Reflect
  绑定——WebCodecs 类 API 在 web-sys 处 unstable 门控，与视频/手柄同法规避）
- `onopen` → Connected + 冲刷 Connecting 期积压；`onmessage` → 帧入队；
  `onclose / onerror` → Closed
- 状态以 **readyState 反射为权威**（onopen/onclose 同时镜像到原子态，
  供 `TcpConn::state()` 同步读取）
- `send`：Connected → `ws.send(Uint8Array)`（同步入浏览器发送缓冲）；
  Connecting → 入队（512 帧上限）

### 与引擎同步 `frame()` 的配合（统一模式，两平台一致）

```rust
struct NetApp { conn: net::TcpConn }
impl Application for NetApp {
    fn frame(&mut self, ctx: &mut Ctx) {
        // 发：随帧发送（Connecting 期自动缓冲）
        let _ = self.conn.send(b"tick");
        // 收：排空本帧到达的所有帧
        while let Some(frame) = self.conn.try_recv() {
            // 处理帧……
        }
    }
}
```

## 五、连接生命周期细节

| 阶段 | 行为 |
|---|---|
| Connecting | 后台握手；send 入队缓冲；**对端不可达 → 10s 超时 → Closed** |
| Connected | 帧化收发；读线程遇错误/对端关闭 → Closed；写线程随 Close/断路退出 |
| Closed | send 报错；try_recv 返回 None；线程全部退出（无泄漏） |

TCP 握手失败与对端正常关闭统一表现为 `Closed` 态（v1 不区分错误原因——
需要时可用 `state()` + 收帧行为侧写，后续可加关闭原因字段）。

## 六、已知边界（如实声明）

1. **原生 TCP 无 TLS（定稿，用户决策：不做）**：`host:port` 为明文传输；
   Web 端 `wss://` 自带 TLS。生产跨网传输建议置于应用层隧道/VPN 之内。
2. **UDP 单次收包缓冲 1500 字节**：超长数据报会被**截断**（v1 按 MTU
   常规报文假设；应用层协议应控制报文 ≤ MTU）。
3. **TCP 入站队列无上限**：长时间不 `try_recv` 会持续积压内存——
   推荐每帧消费；背压策略（上限/丢弃/暂停读）列为观察项。
4. **Android 运行时需 `INTERNET` 权限**（应用 manifest 职责）。
5. **仅客户端**：监听/服务端待需求落地。
6. **Web 端状态双源**：readyState 反射（权威）与镜像原子并存，
   短暂不一致窗口存在但均收敛。
7. **web-sys unstable 门控规避**：WebSocket 绑定为 js_sys Reflect 自持
   （同视频/手柄先例），无编译期类型保护。
