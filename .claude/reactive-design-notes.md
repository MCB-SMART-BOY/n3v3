# 反应式 & 触发器 & 时序 —— 设计笔记

Status: Historical design exploration, not a current syntax/API reference. The shipped subset includes `io.every`, `io.watchFile`, `io.eventNext`, `io.reactive`, `io.liveNext`, `io.retry`, and `io.ensure` (see [`docs/reference/api.md`](../docs/reference/api.md) and [`docs/project/feature-matrix.md`](../docs/project/feature-matrix.md)). The trigger, hook, temporal-constraint, and state-machine syntax below is conceptual pseudocode, not runnable n3v3.
状态：本文是历史设计探索，并非当前语法/API 参考。已交付的子集包括 `io.every`、`io.watchFile`、`io.eventNext`、`io.reactive`、`io.liveNext`、`io.retry` 和 `io.ensure`（见 [`docs/reference/api.md`](../docs/reference/api.md) 与 [`docs/project/feature-matrix.md`](../docs/project/feature-matrix.md)）。下方触发器、钩子、时序约束和状态机语法均为概念伪代码，不能作为 n3v3 程序运行。

写这份文档是要厘清一件事：n3v3 能不能把 "监控-告警-自愈" 这条链路做成语言内置的能力，而不是靠外部脚本拼凑。

---

## 要解决的问题

运维脚本里有几个非常高频的模式：

1. **"当 X 变了，做 Y"** —— 文件变更重启服务、端口 down 了告警
2. **"在 T 时间内必须满足条件 C"** —— 启动后 30s 内必须健康检查通过
3. **"做 A 之前先验证 B，做完之后清理 C"** —— 改配置前备份，改完 reload
4. **"如果重试 N 次还是失败，升级告警"** —— 指数退避、熔断

这些在 Bash 里靠 `while true; do sleep 1; check; done` 硬搓，或者靠 systemd timer / cron。问题是：
- 没有类型检查 —— 拼错变量名静默失败
- 没有取消机制 —— 后台脚本忘了 kill 就一直跑
- 没有组合能力 —— 两个检查脚本想合并？重写

n3v3 有 effect system、有 Task<T>、有 timeout。缺的是把它们串起来的语义层。

---

## Four proposed layers / 拟议的四层能力

The proposal outlines four layers, starting with the most immediately useful.
该方案分四层，从最容易落地的能力开始。

### 第一层：事件流（先落地）

搞清楚"有什么东西发生了变化"。

```text
-- 文件变了
let changes = watchFile(./config.toml);

-- 定时触发
let tick = every(30.seconds);

-- 进程退出
let exits = watchProcess(myApp);
```

这三个是最急的。HTTP health check 放第二期。

事件流的操作（map、filter、merge、debounce）全部 pure —— 它们只是在描述"怎么处理事件"，不产生新的副作用。真正 effectful 的是创建事件源和消费事件。

```text
-- 这些全是 pure
let important = changes.filter(|c| { c.kind == Write });
let deduped  = important.debounce(500.millis);

-- 只有这里才是 effect
deduped.forEach(|_| { reload() });
```

### 第二层：钩子 & 反应（后续）

这一层解决"在什么时候做什么"。

```text
-- before: 可以拦截和修改
before write ./config.toml = |content: String| -> Result<String, String> {
    if !validToml(content) { Err("bad config") }
    else { Ok(content) }
};

-- after: 纯粹副作用，不能拦截
after write ./config.toml = {
    reloadService();
};

-- around: 完全包围，能做回滚
around write ./config.toml = |inner, content| {
    let backup = io.readFile(./config.toml);  -- 先备份
    inner(content);                            -- 执行写入
    if !checkHealth() { io.writeFile(./config.toml, backup); }  -- 不行就回滚
};
```

钩子绑定到作用域生命周期。出了 `scope { }` 自动卸载，不会泄漏。

```text
scope {
    before write ./config.toml = validate;
    -- 只在 scope 内有效
};
-- 出来就没了
```

### 第三层：时序约束（需要前面两层垫着）

"在 5 分钟内、重试 3 次、指数退避" —— 这是运维里最常见的句式。

```text
ensure service.healthy within 5.minutes
    retry 3 times backoff exponential(1.second, 2.0, 30.seconds)
    onViolation { notifyPagerDuty("service down") }
    onTimeout   { notifySlack("health check stuck?") };
```

语法糖，底下就是 `every + filter + timeout`。

```text
-- ensure 本质上是这个的简写：
let check = every(1.second).map(|_| { http.get("/health").status == 200 });
let deadline = now() + 5.minutes;
loop {
    if check.current() { break; }           -- 健康了，退出
    if now() > deadline { onTimeout(); }     -- 超时了
};
```

### 第四层：状态机 & 级联（长期目标）

```text
transition ServiceState {
    Stopped  -> Starting;
    Starting -> Running | Failed(timeout: 30.seconds);
    Running  -> Stopping;
    Stopping -> Stopped | Failed(timeout: 10.seconds);
    _        -> Failed;   -- catch-all
};

cascade service.stop() {
    drainConnections();
    closeSockets();
    notify("stopped");
};
```

---

## Proposed safety model / 拟议安全模型

The following compiler guarantees are design goals, not current guarantees.
以下编译器保证是设计目标，并非当前已交付的保证：

1. **效果隔离**：事件源和消费是 `effect`，变换是 `pure`。纯函数里写 `watchFile` 直接编译报错
2. **生命周期绑定**：钩子跟作用域走，出了 `scope { }` 自动拆。忘不了
3. **循环依赖检测**：`let x = reactive { x.current() + 1 }` 编译期就能发现
4. **状态机穷尽性**：`transition` 没覆盖的状态在类型检查时报警

---

## 跟数据库触发的对比

| 数据库里的 | 在 n3v3 里 |
|-----------|-----------|
| `BEFORE INSERT` — 写入前校验 | `before write file = |x| -> Result<x, Err>` |
| `AFTER UPDATE` — 更新后级联 | `after write file = { reload(); }` |
| `INSTEAD OF` — 完全替换操作 | `around write file = |inner, x| { backup(); inner(x); }` |
| `CHECK (port > 0)` — 值域约束 | `invariant port > 0 && port < 65536` |
| `ON DELETE CASCADE` — 级联删除 | `cascade service.stop() { drain(); close(); }` |
| 状态机（应用层） | `transition ServiceState { ... }` |
| 物化视图 | `reactive { watch x; compute() }` |

数据库触发是行级粒度的。n3v3 不需要行级——系统运维里需要的是文件级、进程级、时间级。

---

## 先做什么

1. `Event<T>` + `watchFile` + `every` —— 这是地基，也是最快能看到价值的部分
2. `map` / `filter` / `debounce` / `merge` —— 事件变换（全部 pure）
3. `reactive` 块 + `Live<T>` 生命周期
4. `before` / `after` / `around` 钩子
5. `ensure` / `require` + 重试
6. `transition` / `cascade` —— 最复杂，最后做

每一步都不大，但每一步都能独立跑通。
