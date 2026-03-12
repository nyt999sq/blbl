# bili-ticker-buy-rust

本项目支持两种运行方式：

- `Tauri 桌面版`（原有路径）
- `Headless Web Server`（新增路径，HTTP + WebSocket + 浏览器前端）

## 1. 代理设置（可选）

如果需要通过 SOCKS5 代理拉取依赖或请求外网：

```bash
export ALL_PROXY=socks5h://127.0.0.1:17890
```

## 2. Headless 运行命令

### 2.1 仅编译检查（推荐先跑）

```bash
cd src-tauri
cargo check --no-default-features --bin headless
```

### 2.2 运行单测（storage data_dir）

```bash
cd src-tauri
cargo test --no-default-features storage::tests::uses_data_dir_override_for_accounts -- --nocapture
```

### 2.3 本地启动（无需 server token）

```bash
cd src-tauri
cargo run --no-default-features --bin headless -- serve --host 127.0.0.1 --port 18080 --data-dir ./data
```

### 2.4 公网启动（必须提供 server token）

```bash
cd src-tauri
cargo run --no-default-features --bin headless -- serve --host 0.0.0.0 --port 18080 --token YOUR_TOKEN --data-dir ./data
```

### 2.5 鉴权快速检查

1) 获取 session（无 token 模式）：

```bash
curl -X POST http://127.0.0.1:18080/api/auth/token-login
```

2) 获取 session（有 token 模式）：

```bash
curl -X POST -H "Authorization: Bearer YOUR_TOKEN" http://127.0.0.1:18080/api/auth/token-login
```

3) 访问受保护接口（示例）：

```bash
curl -H "x-session-token: YOUR_SESSION" http://127.0.0.1:18080/api/accounts
```

## 3. 前端（Web）命令

### 3.1 构建

```bash
npm run build
```

### 3.2 开发模式（代理到 headless API）

```bash
VITE_RUNTIME=web VITE_API_PROXY_TARGET=http://127.0.0.1:18080 npm run dev
```

## 4. 桌面版（Tauri）命令

### 4.1 编译检查

```bash
cd src-tauri
cargo check --features desktop --bin bili-ticker-buy-rust
```

### 4.2 开发运行

```bash
npm run tauri dev
```

## 5. 说明

- headless 路径建议使用 `--no-default-features`，避免不必要的桌面依赖编译。
- 当 `--host` 不是 `127.0.0.1 / localhost / ::1` 时，`--token` 为必填。
- `data_dir` 不指定时默认使用 `./data`（headless）。
