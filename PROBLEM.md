# 项目改进建议

## 🔴 高优先级（安全/正确性）

### 1. Token 泄漏到日志

- `command_parser.rs:220` 的 `Display` 实现打印了明文 token
- `ConnectionUrls::Display` 中的 URL 包含 `?token=...`，同样泄漏
- `callbacks/mod.rs:43` 打印了完整的回调消息原文，可能暴露远程命令内容
- **建议**: 在 `Args::Display` 和 `ConnectionUrls::Display` 中脱敏 token；回调日志只打印消息类型

### 2. 缺少 feature 编译守卫

- 当 `ureq-support` 和 `nyquest-support` 都未启用时，`BasicInfo::push`、`exec_command`、ping HTTP 等函数会静默不做任何事
- **建议**: 在 `main.rs` 中添加：
  ```rust
  #[cfg(not(any(feature = "ureq-support", feature = "nyquest-support")))]
  compile_error!("Enable at least one HTTP transport feature: `ureq-support` or `nyquest-support`.");
  ```

### 3. 回调任务生命周期泄漏

- `main.rs:141` spawn 的 callback task 的 `JoinHandle` 被赋给 `_listener` 后丢弃，重连时旧任务不会被中止，可能导致任务泄漏
- `callbacks/pty.rs:130` 的 `tokio::select!` 结束后未 abort 另一个 sibling task
- **建议**: 保留 `JoinHandle`，在重连/session 结束时调用 `abort()`

### 4. 阻塞 I/O 在 async 上下文中执行

- `BasicInfo::push`（`data_struct.rs:66-119`）使用阻塞 HTTP 客户端
- ICMP ping（`ping.rs:189-354`）是同步阻塞的，最多持有 Tokio 工作线程 3 秒
- IP 查询（`ip.rs:30-148`）中的 HTTP 请求也是阻塞的
- **建议**: 用 `tokio::task::spawn_blocking` 包装所有阻塞调用

### 17. netlink 连接数统计把结束/错误消息也算进结果

- `get_info/network/netlink.rs:149-153` 在判断消息类型前先执行了 `msgs += 1`
- `get_info/network/netlink.rs:176-177` 将 `NLMSG_DONE` 和 `NLMSG_ERROR` 一并视为“正常结束”
- **影响**: TCP/UDP 连接数会系统性偏大，错误响应还会被误报为成功统计
- **建议**: 先判断消息类型再计数；对 `NLMSG_ERROR` 解析 errno 并返回 `Err`

### 18. netlink header 解析存在未对齐解引用的 UB 风险

- `get_info/network/netlink.rs:168` 使用 `&*(b.as_ptr() as *const libc::nlmsghdr)` 直接解引用原始指针
- 在部分架构上该地址可能未对齐
- **影响**: 可能导致崩溃或错误解析，尤其是在 ARM 等对齐要求更严格的平台上
- **建议**: 改用 `ptr::read_unaligned`，或先拷贝到对齐缓冲区再解析

### 19. 回调处理没有并发上限，容易被消息洪泛拖垮

- `callbacks/mod.rs:56-72`、`callbacks/mod.rs:80-99`、`callbacks/mod.rs:107-128` 对每条消息都直接 `tokio::spawn`
- **影响**: 服务端或中间链路异常时会导致任务数量无限增长，形成明显 DoS 面
- **建议**: 增加 `Semaphore` 并发限制、队列背压，并对不同回调类型做限流/去重

### 20. IPv6 TCP ping 地址拼接错误

- `callbacks/ping.rs:35-57` 将 host/port 手工拼接为 `"{ip}:{port}"`
- `callbacks/ping.rs:92-95` 对 IPv6 地址会形成 `::1:80` 这类非法 socket 地址
- **影响**: IPv6 TCP ping 基本不可用，会被持续误报为连接失败
- **建议**: 改用 `SocketAddr` / `ToSocketAddrs` 统一构造目标地址

### 21. 同时启用两个 HTTP backend 会重复发送请求或覆盖结果

- `data_struct.rs:68-118` 的 BasicInfo 上报在两个 feature 同时启用时会发送两次
- `callbacks/exec.rs:69-101` 的命令回调也会重复发送
- `callbacks/ping.rs:129-141` 的 HTTP ping 会执行两个实现，后一个结果会覆盖前一个
- **影响**: 服务端可能收到重复上报；前一个实现成功、后一个失败时，最终结果仍可能被错误地视为失败
- **建议**: 将 `ureq-support` 与 `nyquest-support` 设计为互斥，或实现明确的主后备策略

---

## 🟡 中优先级（正确性/跨平台）

### 5. `exec.rs` 硬编码 `bash` 作为 shell

- `callbacks/exec.rs:32` 固定使用 `bash -c`，在 Windows 或没有 bash 的系统上会失败
- **建议**: 根据平台选择 shell（Windows → `cmd.exe /C`，Unix → `sh -c`），或复用 `args.terminal_entry`

### 6. `main.rs` 重复的 clippy allow

- `main.rs:5-6` 两行都是 `clippy::cast_precision_loss`，重复了
- **建议**: 删除重复行

### 7. `network_saver` 使用魔数作为状态标识

- `network_saver.rs` 中用 `i64::MIN`、`i64::MIN+1`、`i64::MIN+2` 作为 offset 的哨兵值，极其脆弱且难以理解
- **建议**: 改用枚举表达状态：
  ```rust
  enum OffsetState {
      Valid(i64, i64),
      Recalculate,
      RebootWithinCycle,
      InitialCycle,
  }
  ```

### 8. `command_parser.rs` 中 `disable_network_statistics` 逻辑是空操作

- `command_parser.rs:187-193` 的三分支判断最终等价于 `self.disable_network_statistics`，中间的条件判断毫无意义
- **建议**: 直接使用 `self.disable_network_statistics`

### 9. `ping.rs` 中检查 `USER == "root"` 不可靠

- `callbacks/ping.rs:67` 通过环境变量检查是否为 root 来决定能否创建 raw socket
- 有 `CAP_NET_RAW` capability 的非 root 用户也可以创建 raw socket，且环境变量可被篡改
- **建议**: 直接尝试创建 socket，失败时返回 OS 错误

### 10. `build_urls` 在不支持的 scheme 上 panic

- `utils.rs:62` 函数签名返回 `Result`，但在遇到不支持的 scheme 时使用了 `panic!`
- **建议**: 改为返回 `Err`

### 22. 远程命令执行缺少超时控制

- `callbacks/exec.rs:31-52` 执行远程命令时没有超时或取消逻辑
- **影响**: 恶意或异常命令可以长期挂起，占用子进程、任务和系统资源
- **建议**: 使用 `tokio::time::timeout` 包裹执行流程，并在超时后主动终止子进程

### 23. token 与 request_id 直接拼接到 URL 查询参数中

- `utils.rs:69-72` 将 `token` 直接拼接到 query string
- `callbacks/pty.rs:21-24` 将 `request_id` 直接拼接到 PTY WebSocket URL
- **影响**: 值中若包含 `&`、`=`、`#`、空格或 `%`，URL 会被破坏，导致鉴权失败、参数串扰或会话建立异常
- **建议**: 使用 `url::Url` 的 `query_pairs_mut().append_pair(...)` 统一追加查询参数

### 24. 回调 WebSocket 读取错误被静默吞掉

- `callbacks/mod.rs:34-37` 在 `reader.next()` 返回 `Err` 时直接 `continue`
- **影响**: 回调链路损坏后主上报可能仍继续运行，形成“实时上报正常但 exec/ping/terminal 失效”的半故障状态
- **建议**: 读取错误时退出回调循环并触发主连接重建

### 25. `--tls` 与 URL scheme 同时控制传输层，语义冲突

- `command_parser.rs:32-38` 提供了 `--tls` 开关
- `utils.rs:45-81` 又根据 `http_server` / `ws_server` 的 scheme 构造实际连接地址
- `main.rs:117-123` 在建立 WebSocket 连接时再次使用 `args.tls`
- **影响**: 配置来源不唯一，容易出现 `http://... --tls` 仍走明文 HTTP 上报、但 WebSocket 走 TLS 的不一致行为
- **建议**: 统一以 URL scheme 或 `--tls` 为单一真相，不要让两个来源同时决定传输层行为

### 26. 运行间隔参数缺少下界校验，可能触发紧循环

- `command_parser.rs:61-63` 的 `realtime_info_interval` 允许为 0
- `command_parser.rs:84-95` 的 `network_interval`、`network_interval_number` 也缺少下界校验
- `main.rs:195-199` 与 `get_info/network/network_saver.rs:212-242` 中会直接消费这些值
- **影响**: 将间隔设为 0 时会形成高速循环，迅速打满 CPU、网络、日志或磁盘
- **建议**: 在 CLI 解析阶段强制这些参数 `>= 1`

### 27. 进程数统计实现是 Linux 专用硬编码

- `get_info/mod.rs:21-33` 直接读取 `/proc` 目录统计进程数
- **影响**: 在 Windows、macOS、BSD 等平台上会静默返回 0，造成上报数据失真
- **建议**: 改为使用 `sysinfo` 的进程列表，或至少做平台分支处理

### 28. WebSocket 连接错误信息被过度抹平

- `utils.rs:91-121` 将底层连接错误统一映射为几个固定字符串
- **影响**: DNS 失败、TLS 握手失败、证书错误、HTTP upgrade 失败等问题难以区分，排障成本高
- **建议**: 保留底层错误详情，至少将原始错误拼接到返回信息中

---

## 🟢 低优先级（代码质量/重复）

### 11. `dry_run.rs` 重复了网络过滤关键字列表

- `dry_run.rs:56-62` 定义了一份和 `network/mod.rs:157-159` 完全相同的过滤关键字
- **建议**: 复用 `filter_network()` 函数或将关键字列表提取为共享常量

### 12. `icmp_ipv4` / `icmp_ipv6` 大量复制粘贴

- `callbacks/ping.rs` 中两个函数约 90% 代码相同
- **建议**: 提取泛型函数统一处理 IPv4/IPv6

### 13. `data_struct.rs` 中 `fake` 乘数样板代码过多

- `RealTimeInfo::build` 中每个字段都重复 `(x as f64 * fake) as u64` 模式
- **建议**: 提取 helper 函数，如 `fn scale(value: u64, factor: f64) -> u64`

### 14. Windows toast 的 `.expect()` 会导致程序崩溃

- `main.rs:103` 使用 `.expect()` 处理 toast 通知结果，通知失败会直接 panic
- **建议**: 改用 `if let Err(e)` 记录错误日志并继续运行

### 15. `cpu.rs` 缺少除零保护

- `cpu.rs:44` 计算 CPU 平均使用率时，如果 `cpus()` 返回空列表会产生 NaN
- **建议**: 添加 `if cpus.is_empty() { return Cpu { usage: 0.0 }; }` 守卫

### 16. `exec.rs` 的 `wait_with_output()` 无大小限制

- `callbacks/exec.rs:42` 调用 `wait_with_output()` 会将子进程全部 stdout/stderr 读入内存
- 恶意或错误的命令可能导致无限内存增长
- **建议**: 对输出大小设置上限，超出后截断

### 29. CPU 名称去重后的输出顺序不稳定

- `get_info/cpu.rs:20-29` 使用 `HashSet` 去重后直接 `join`
- **影响**: 多 CPU 品牌字符串在不同运行间可能顺序不同，不利于日志比对和缓存命中
- **建议**: 在去重后执行稳定排序，再生成最终字符串
