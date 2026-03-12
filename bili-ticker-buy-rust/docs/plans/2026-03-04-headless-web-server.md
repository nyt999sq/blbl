# Headless Web Server Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 在不破坏现有 Tauri 桌面版的前提下，新增可在无桌面 Linux 上运行的 headless Web Server（HTTP + WebSocket + 内置静态托管），并让现有前端可在浏览器直接登录与操作。

**Architecture:** 保留 `src-tauri/src/main.rs` 作为桌面入口；新增 `src-tauri/src/bin/headless.rs` 作为服务器入口。将下单与事件推送从 `tauri::Window` 解耦为 `TaskEventSink` trait，Tauri 与 WebSocket 分别提供实现。前端通过 `platform` 适配层屏蔽 tauri/web 运行时差异，App 业务状态与 UI 尽量不变。

**Tech Stack:** Rust (`axum`, `tokio`, `clap`, `serde`, `reqwest`), WebSocket (axum ws), React/Vite, JSON 文件存储。

---

### Task 1: 建立基线与入口骨架

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/bin/headless.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/lib.rs`（编译验证）

**Step 1: 写失败验证（编译层）**

```bash
cd src-tauri
cargo check --bin headless
```

Expected: FAIL（headless 入口/依赖尚未存在）。

**Step 2: 实现最小骨架**

- 在 `Cargo.toml` 添加 headless 所需依赖：`axum`、`tower-http`、`clap`。
- 新建 `lib.rs` 统一导出 `api/auth/buy/storage/util` 等模块，供两个二进制复用。
- 新建 `bin/headless.rs`，仅实现命令行解析与占位启动日志。
- 调整 `main.rs` 从库模块引用，保持桌面行为不变。

**Step 3: 绿灯验证**

```bash
cd src-tauri
cargo check --bin headless --bin bili-ticker-buy-rust
```

Expected: PASS。

**Step 4: 提交**

```bash
git add src-tauri/Cargo.toml src-tauri/src/lib.rs src-tauri/src/bin/headless.rs src-tauri/src/main.rs
git commit -m "feat: add headless binary scaffold and shared rust library layout"
```

### Task 2: 抽离 TaskEventSink 并保持桌面回归

**Files:**
- Create: `src-tauri/src/core/events.rs`
- Modify: `src-tauri/src/buy.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/core/events.rs`（类型/默认实现测试）

**Step 1: 写失败测试**

- 增加一个最小 sink mock 测试，验证 `emit_log/payment_qrcode/task_result` 能被统一调用与记录。

Run:

```bash
cd src-tauri
cargo test core::events -- --nocapture
```

Expected: FAIL（trait 与测试目标尚未落地）。

**Step 2: 最小实现**

- 定义 `TaskEvent` 与 `TaskEventSink` trait。
- 改 `buy.rs`：`start_buy_task` 入参从 `Window` 改为 `Arc<dyn TaskEventSink + Send + Sync>`。
- Tauri 侧新增 `TauriEventSink`，内部继续 `window.emit("log"/"payment_qrcode"/"task_result", ...)`。

**Step 3: 绿灯验证**

```bash
cd src-tauri
cargo test core::events
cargo check --bin bili-ticker-buy-rust
```

Expected: PASS，桌面入口可编译。

**Step 4: 提交**

```bash
git add src-tauri/src/core/events.rs src-tauri/src/buy.rs src-tauri/src/main.rs
git commit -m "refactor: decouple buy task events via TaskEventSink"
```

### Task 3: 数据目录注入（兼容 JSON 结构）

**Files:**
- Modify: `src-tauri/src/storage.rs`
- Create: `src-tauri/src/core/storage.rs`
- Test: `src-tauri/src/storage.rs`（`data_dir` 下读写）

**Step 1: 写失败测试**

- 新增测试：临时目录下写入/读取 `accounts.json`、`history.json`、`project_history.json`。

Run:

```bash
cd src-tauri
cargo test storage::tests:: -- --nocapture
```

Expected: FAIL（当前无法注入目录）。

**Step 2: 最小实现**

- 给 storage 增加“可选全局 data_dir 覆盖”，默认仍为 tauri app config dir（桌面兼容）。
- 增加 `set_data_dir(PathBuf)` 供 headless 启动时注入。
- 对现有 JSON 结构完全不做字段变更。

**Step 3: 绿灯验证**

```bash
cd src-tauri
cargo test storage::tests::
```

Expected: PASS。

**Step 4: 提交**

```bash
git add src-tauri/src/storage.rs src-tauri/src/core/storage.rs
git commit -m "feat: support configurable data directory while keeping json schema"
```

### Task 4: Headless HTTP + WS 最小可用链路

**Files:**
- Create: `src-tauri/src/headless/mod.rs`
- Create: `src-tauri/src/headless/router.rs`
- Create: `src-tauri/src/headless/auth.rs`
- Create: `src-tauri/src/headless/ws.rs`
- Create: `src-tauri/src/headless/handlers.rs`
- Modify: `src-tauri/src/bin/headless.rs`
- Test: `src-tauri/src/headless/auth.rs`（token/session）

**Step 1: 写失败测试**

- 覆盖：`token-login` 成功/失败、缺 token 拒绝、session 校验。

Run:

```bash
cd src-tauri
cargo test headless::auth::tests::
```

Expected: FAIL。

**Step 2: 最小实现**

- CLI: `headless serve --host --port --token --data-dir`，校验“非 127.0.0.1 必须带 token”。
- HTTP:
  - `POST /api/auth/token-login`
  - `GET /api/login/qrcode`
  - `GET /api/login/poll`
  - `POST /api/accounts/import-cookie`
  - `GET /api/accounts`
  - `DELETE /api/accounts/:uid`
  - `POST /api/project/fetch`
  - `POST /api/project/buyers`
  - `POST /api/project/addresses`
  - `POST /api/time/sync`
  - `POST /api/task/start`
  - `POST /api/task/stop`
  - `GET /api/history`
  - `GET /api/project-history`
  - `POST /api/project-history`
  - `DELETE /api/project-history`
- WS:
  - `GET /api/ws?session=...`
  - 服务端单向推送 `log/payment_qrcode/task_result`。

**Step 3: 绿灯验证**

```bash
cd src-tauri
cargo test headless::auth::tests::
cargo check --bin headless
```

Expected: PASS。

**Step 4: 提交**

```bash
git add src-tauri/src/headless src-tauri/src/bin/headless.rs
git commit -m "feat: add headless axum server with token auth and websocket events"
```

### Task 5: 前端平台适配层（最小侵入）

**Files:**
- Create: `src/platform/runtime.ts`
- Create: `src/platform/apiClient.ts`
- Create: `src/platform/eventClient.ts`
- Create: `src/platform/notificationClient.ts`
- Modify: `src/App.jsx`
- Modify: `src/main.jsx`
- Modify: `vite.config.js`
- Test: `src/platform/*.ts`（基础单测或运行时 smoke）

**Step 1: 写失败验证**

```bash
npm run build
```

Expected: FAIL（适配层未接入前，web 模式无法工作或类型/引用未满足）。

**Step 2: 最小实现**

- `apiClient.call(command, payload)`：
  - tauri 运行时走 `invoke`。
  - web 运行时映射到 `/api/*` HTTP。
- `eventClient.on(type, handler)`：
  - tauri 走 `listen`。
  - web 走 WebSocket 订阅。
- 通知：
  - tauri 保持原逻辑。
  - web 走 `Notification` API，失败静默降级。
- `App.jsx` 将 `invoke/listen/sendNotification` 替换为适配层调用。

**Step 3: 绿灯验证**

```bash
npm run build
```

Expected: PASS。

**Step 4: 提交**

```bash
git add src/platform src/App.jsx src/main.jsx vite.config.js
git commit -m "refactor: add runtime adapters for tauri and web headless modes"
```

### Task 6: 回归与验收脚本

**Files:**
- Modify: `README.md`（若存在）
- Create: `docs/headless-usage.md`

**Step 1: 执行验证命令**

```bash
cd src-tauri && cargo check --bin bili-ticker-buy-rust --bin headless
cd src-tauri && cargo test
cd .. && npm run build
```

Expected: 全部 PASS。

**Step 2: 手工验收（记录结果）**

- 桌面：登录、启动任务、停止任务、支付二维码、历史记录。
- 服务器：`headless serve` 启动、token 登录、WS 日志推送、重启后数据恢复。

**Step 3: 提交文档**

```bash
git add docs/headless-usage.md README.md
git commit -m "docs: add headless deployment and verification guide"
```
