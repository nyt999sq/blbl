## biliTickerBuy 本地运行说明

这个仓库里真正可运行的项目在 `bili-ticker-buy-rust/` 子目录，当前支持两种形态：

- `Tauri 桌面版`
- `Headless Web Server + 浏览器前端`

如果你想在本地跑“网页版”，不要在仓库根目录直接执行 `npm run tauri dev`。正确方式是进入 `bili-ticker-buy-rust/`，先启动 Rust 的 headless 服务，再启动 Vite 前端。

## 目录说明

### 仓库结构

- `README.md`
  当前这份总入口说明。
- `bili-ticker-buy-rust/`
  实际项目目录，前端、Rust 后端、Vite 配置都在这里。
- `bili-ticker-buy-rust/src/`
  React 前端。
- `bili-ticker-buy-rust/src-tauri/`
  Rust 代码。`src/bin/headless.rs` 是网页版服务入口。
- `bili-ticker-buy-rust/src/platform/`
  Web/Tauri 运行时适配层。网页版请求会通过这里转到 `/api/*`。

## 服务器部署说明

当前线上服务部署在：

- 服务器：`47.101.38.217`
- 目录：`/root/blbl/bili-ticker-buy-rust`
- 访问地址：`http://47.101.38.217:18080/`
- `systemd` 服务名：`bili-ticker-buy-headless.service`

### 线上目录形态

服务器上已经整理为“仅保留运行所需文件”的形态，不再保留源码、`node_modules`、Cargo 工程和前端构建配置。当前线上目录只包含：

- `dist/`
  前端静态资源。
- `runtime/headless`
  Rust 编译后的 headless 可执行文件。
- `data/`
  运行时数据目录，当前会保存：
  - `accounts.json`
  - `history.json`
  - `project_history.json`
  - `share_presets.json`
- `.headless_token`
  headless 服务的 token 文件。

### 当前 systemd 启动方式

线上服务不是通过源码直接运行，而是通过 `systemd` 启动编译后的二进制：

```bash
systemctl status bili-ticker-buy-headless.service
```

服务会实际执行：

```bash
/root/blbl/bili-ticker-buy-rust/runtime/headless serve \
  --host 0.0.0.0 \
  --port 18080 \
  --token "$(cat /root/blbl/bili-ticker-buy-rust/.headless_token)" \
  --data-dir /root/blbl/bili-ticker-buy-rust/data
```

### 常用线上运维命令

查看状态：

```bash
systemctl status bili-ticker-buy-headless.service
```

重启服务：

```bash
systemctl restart bili-ticker-buy-headless.service
```

查看日志：

```bash
journalctl -u bili-ticker-buy-headless.service -f
```

### 重新部署说明

由于线上目录现在是 runtime-only 形态，后续如果需要更新功能，应在本地仓库完成：

1. 修改源码
2. 本地验证
3. 重新构建前端 `dist`
4. 重新编译 `headless`
5. 将新的 `dist/` 和 `runtime/headless` 上传到服务器

不建议直接在服务器上继续保留一整套源码开发环境。

### 本地网页版的运行原理

本地 Web 模式不是“只跑一个前端页面”，而是两部分协作：

1. `headless` Rust 服务
   提供 HTTP API、WebSocket 事件推送，并负责读写本地数据。
2. `Vite` 前端开发服务器
   提供 React 页面，开发时把 `/api` 和 WebSocket 代理到 headless 服务。

默认端口如下：

- 前端：`http://localhost:1420`
- 后端：`http://127.0.0.1:18080`

## 环境准备

### 必需软件

- `Node.js`
  建议 18 或更高版本。
- `npm`
  随 Node.js 一起安装。
- `Rust`
  需要 `rustc` 和 `cargo`。

### 环境检查

在任意终端执行：

```bash
node -v
npm -v
rustc --version
cargo --version
```

如果其中任意一个命令不存在，先补齐环境再继续。

### 可选代理

如果拉依赖或请求外网较慢，可以先设置代理：

```bash
export ALL_PROXY=socks5h://127.0.0.1:17890
```

## 本地网页版开发模式

这是最推荐的方式。优点是前端有热更新，改完页面马上能看到结果。

### 第 1 步：进入项目目录并安装依赖

```bash
cd bili-ticker-buy-rust
npm install
```

### 第 2 步：检查 headless 后端能否编译

```bash
cd src-tauri
cargo check --no-default-features --bin headless
```

这一步通过，说明 Rust 的网页版入口能正常编译。

### 第 3 步：终端 A 启动后端

在一个新的终端窗口执行：

```bash
cd /Users/ningshen/Downloads/biliTickerBuy-main/bili-ticker-buy-rust/src-tauri
cargo run --no-default-features --bin headless -- serve --host 127.0.0.1 --port 18080 --data-dir ./data
```

看到类似下面的输出就表示后端启动成功：

```text
headless server listening on 127.0.0.1:18080 data_dir=./data
```

#### 说明

- 这里使用的是本地地址 `127.0.0.1`，所以**不需要** `--token`。
- `./data` 会被解析成当前目录下的数据目录，也就是：
  `bili-ticker-buy-rust/src-tauri/data/`
- 这个目录会保存账号、历史记录等本地数据。

### 第 4 步：终端 B 启动前端

再开一个新的终端窗口执行：

```bash
cd /Users/ningshen/Downloads/biliTickerBuy-main/bili-ticker-buy-rust
VITE_RUNTIME=web VITE_API_PROXY_TARGET=http://127.0.0.1:18080 npm run dev
```

正常情况下会看到类似输出：

```text
VITE v5.x.x  ready in xxx ms
Local: http://localhost:1420/
```

### 第 5 步：浏览器访问页面

打开：

```text
http://localhost:1420
```

第一次打开时，页面可能会弹出一个提示框：

```text
请输入服务器 Token（若未设置可留空）
```

本地模式下，因为后端没有设置 `--token`，这里**直接留空即可**。

### 第 6 步：验证后端接口是否正常

如果页面打不开，先验证后端：

```bash
curl -X POST http://127.0.0.1:18080/api/auth/token-login
```

正常会返回一个 JSON，里面包含 `session`，例如：

```json
{"session":"xxxxx","expires_at":1773322225}
```

继续验证一个需要登录态的接口：

```bash
SESSION="上一步返回的 session"
curl -H "x-session-token: $SESSION" http://127.0.0.1:18080/api/accounts
```

如果这里能返回数组，说明 headless 后端是通的，前端问题通常就在代理、端口或浏览器缓存。

## 本地单进程预览模式

如果你不需要热更新，只想像部署环境那样在本地“打开就用”，可以用这个模式。

### 第 1 步：先构建前端静态资源

```bash
cd /Users/ningshen/Downloads/biliTickerBuy-main/bili-ticker-buy-rust
npm run build
```

构建完成后，前端文件会落到：

```text
bili-ticker-buy-rust/dist/
```

### 第 2 步：启动 headless 服务

```bash
cd /Users/ningshen/Downloads/biliTickerBuy-main/bili-ticker-buy-rust/src-tauri
cargo run --no-default-features --bin headless -- serve --host 127.0.0.1 --port 18080 --data-dir ./data
```

这个服务会优先读取上一级目录的 `../dist`，也就是刚才构建出来的前端资源。

### 第 3 步：直接访问后端端口

打开：

```text
http://127.0.0.1:18080
```

这时不需要再单独启动 `npm run dev`。

## 带 Token 的本地模式

如果你想模拟服务器环境，也可以在本地启动时加 Token：

### 启动后端

```bash
cd /Users/ningshen/Downloads/biliTickerBuy-main/bili-ticker-buy-rust/src-tauri
cargo run --no-default-features --bin headless -- serve --host 127.0.0.1 --port 18080 --token YOUR_TOKEN --data-dir ./data
```

### 启动前端

```bash
cd /Users/ningshen/Downloads/biliTickerBuy-main/bili-ticker-buy-rust
VITE_RUNTIME=web \
VITE_API_PROXY_TARGET=http://127.0.0.1:18080 \
VITE_HEADLESS_SERVER_TOKEN=YOUR_TOKEN \
npm run dev
```

这样浏览器就不会再反复弹框要求你手输 Token。

## 停止服务

### 停止开发模式

- 停止后端：在终端 A 按 `Ctrl+C`
- 停止前端：在终端 B 按 `Ctrl+C`

### 清理浏览器里的本地缓存

如果你之前输入过错误的 Token，页面可能会持续报鉴权失败。可以在浏览器控制台执行：

```js
localStorage.removeItem("bili_headless_server_token");
localStorage.removeItem("bili_headless_session");
```

然后刷新页面重新登录。

## 常见问题

### 为什么在仓库根目录执行 `npm run tauri dev` 会报错？

因为根目录不是 Node 项目，真正的 `package.json` 在：

```text
bili-ticker-buy-rust/package.json
```

所以必须先：

```bash
cd bili-ticker-buy-rust
```

### 为什么页面能打开，但接口全是 404？

通常是前端没有带上 Web 运行时参数。你启动前端时必须包含：

```bash
VITE_RUNTIME=web VITE_API_PROXY_TARGET=http://127.0.0.1:18080 npm run dev
```

少了这两个变量，`/api` 代理不会生效。

### 为什么页面一直提示输入 Token？

这是当前前端实现的行为：如果浏览器里没有缓存过 Token，它会先询问一次。

- 本地无 Token 模式：直接留空
- 本地有 Token 模式：输入你启动后端时传入的 `--token`

如果之前输错了，先清理 `localStorage` 再刷新。

### `18080` 或 `1420` 端口被占用了怎么办？

你可以改端口，但前后端要一起改：

#### 改后端端口

```bash
cargo run --no-default-features --bin headless -- serve --host 127.0.0.1 --port 18081 --data-dir ./data
```

#### 改前端代理目标

```bash
VITE_RUNTIME=web VITE_API_PROXY_TARGET=http://127.0.0.1:18081 npm run dev
```

前端自己的开发端口 `1420` 是在 `vite.config.js` 里固定的。如果 `1420` 已被占用，需要先释放端口，或者修改 `bili-ticker-buy-rust/vite.config.js` 里的 `server.port`。

### 为什么没有账号数据？

本地 Web 版使用的是 `src-tauri/data/` 下的数据。首次运行如果这个目录为空，账号列表就是空的，需要你：

- 在页面里扫码登录
- 或者导入已有 Cookie

### 想运行桌面版怎么办？

网页版和桌面版都在同一个项目里。桌面版命令如下：

```bash
cd /Users/ningshen/Downloads/biliTickerBuy-main/bili-ticker-buy-rust
npm run tauri dev
```

但这和本文的“本地网页版”不是同一条路径。

## 推荐的最短命令清单

如果你只想最快跑起来，按下面两条命令分别在两个终端执行：

### 终端 A

```bash
cd /Users/ningshen/Downloads/biliTickerBuy-main/bili-ticker-buy-rust/src-tauri
cargo run --no-default-features --bin headless -- serve --host 127.0.0.1 --port 18080 --data-dir ./data
```

### 终端 B

```bash
cd /Users/ningshen/Downloads/biliTickerBuy-main/bili-ticker-buy-rust
VITE_RUNTIME=web VITE_API_PROXY_TARGET=http://127.0.0.1:18080 npm run dev
```

然后打开：

```text
http://localhost:1420
```
