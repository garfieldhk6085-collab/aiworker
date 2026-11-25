# aiworker

个人效率型、本地优先、Web 自托管的 AI 驱动自动化任务管理平台。

## 技术栈
- **后端**：Rust (`Axum` + `Tokio` + `SQLx` + `SQLite WAL` + **`rig-core 0.47`**)
- **前端**：React 19 + Vite + TypeScript + Tailwind CSS v4

## 核心能力
- **多种触发方式**：自然语言即时派发、标准 Cron 定时调度、以及任务专属 **Inbound Webhook**（`POST /api/hooks/{token}`，可对接 Siri 快捷指令、GitHub Webhook 等）。
- **Draft-first 人工审批（Human-in-the-Loop）**：读取类工具自动放行；文件写入、Python/Shell 沙箱执行、外部 HTTP 请求默认暂停并推送至 Today 收件箱，审批后从 SQLite 断点无缝恢复。
- **Python 3 + Linux Shell 隔离沙箱**：优先使用 Docker 隔离容器（限内存/CPU/PIDs/可选断网，仅挂载 `workspace/`），无 Docker 时自动降级为受控本地子进程（`env_clear` + 强超时）。
- **Skills 技能包（`SKILL.md`）**：按需加载（Progressive Disclosure），以标准 Markdown + 配套脚本替代沉重的 MCP 进程运维，省 Token 且易扩展。
- **Worker 角色预设 & 个人记忆库**：支持为不同场景绑定不同人设、模型（`rig-core` 原生支持 OpenAI / Anthropic Claude / Google Gemini / DeepSeek / Ollama / Mock）与工具白名单。
- **工作区文件收发 & Secrets 保险箱**：Web 端直接上传待处理文件、下载 AI 产物；Secrets 仅注入沙箱环境变量，日志自动打码脱敏。
- **生产级稳定性与异步触达**：启动崩溃恢复、LLM 指数退避重试、并发限流、每日 Token 预算熔断、日志自动清理、飞书/Slack/Discord/通用 Webhook 异步推送，以及可选单用户 Bearer Token 鉴权。

## 本地开发启动

### 1. 启动 Rust 后端（默认端口 `3001`）
```bash
cd backend
cargo run
```

### 2. 启动 React 前端（默认端口 `5173`，已配置 `/api` 代理）
```bash
cd frontend
pnpm install
pnpm dev
```

打开 `http://localhost:5173` 即可使用。

## 生产自托管（单进程托管前后端）
先构建前端静态产物，再直接运行 Rust 后端即可（后端会自动探测并托管 `../frontend/dist`）：
```bash
cd frontend && pnpm build
cd ../backend && AIWORKER_AUTH_TOKEN="your-secret-token" cargo run --release
```

### 常用环境变量
| 变量名 | 默认值 | 说明 |
| :--- | :--- | :--- |
| `PORT` | `3001` | 监听端口 |
| `AIWORKER_AUTH_TOKEN` | 空（免密） | 设置后对所有 `/api/*` 启用 Bearer Token 保护 |
| `AIWORKER_DATA_DIR` | `./data` | SQLite 数据库目录 |
| `AIWORKER_WORKSPACE_DIR` | `./workspace` | AI 沙箱工作区与 `skills/` 根目录 |
| `AIWORKER_STATIC_DIR` | `../frontend/dist` | 前端构建产物托管目录 |
