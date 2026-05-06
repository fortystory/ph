# Project Hub (ph) — Claude 指南

## 🧭 项目概览

这是一个用 Rust 编写的**本地优先 AI 项目管理中枢**。

系统架构为：

- **单一二进制文件 (`ph`)**
- 操作于**数据目录 (`~/.project-hub`)**
- 提供：
  - CLI 界面
  - （未来）Web UI
  - AI Agent 执行

---

## 🧱 架构原则

### 1. 单一二进制

所有功能必须包含在同一个可执行文件内：

- CLI
- Web 服务器
- Agent 运行时

禁止拆分为多个服务。

---

### 2. 外部状态（数据目录）

所有运行时数据都存储在二进制文件之外：

```
~/.project-hub/
```

包含：

- `projects.toml`
- `agents/`
- `prompts/`
- `knowledge/`
- `workspace/`（软链接）
- `todos.db`
- `docs/`（todo 工作流文档）

⚠️ 禁止在二进制中硬编码数据。

---

### 3. 存储规则

| 类型        | 存储方式 |
|------------|---------|
| Projects   | TOML    |
| Agents     | TOML    |
| Prompts    | 文件    |
| Todos      | SQLite  |
| Workspace  | 软链接  |
| Docs       | Markdown |
| Knowledge  | 文件 + Qdrant（向量） |
| Sync Log   | SQLite  |

---

### 4. 分层设计

必须严格遵守分层：

```
CLI / Web
↓
Service 层
↓
Infra 层
↓
Storage（文件系统 / SQLite）
```

❗ 规则：

- CLI 禁止直接访问 infra
- 业务逻辑必须放在 `service` 中
- `core` 只包含数据结构

---

## 📁 仓库结构

```
crates/
├── core/       # 仅数据模型
├── infra/      # 文件系统、SQLite、Qdrant、MCP 服务器
│   ├── db.rs               # 数据库初始化与 schema
│   ├── git.rs              # Git 操作（worktree、commits）
│   ├── knowledge.rs        # 知识库文件加载
│   ├── knowledge_index.rs  # Qdrant 向量索引
│   ├── knowledge_capture.rs # 知识捕获（对话、实时）
│   ├── mcp_server.rs       # MCP stdio 服务器
│   └── ...
├── service/    # 业务逻辑
│   ├── knowledge_sync.rs   # 知识同步（git、对话）
│   └── ...
└── cli/        # 命令行界面
```

---

## 🧩 核心概念

### Project（项目）

代表一个受管理的代码库：

- id
- name
- path
- agent
- prompt
- knowledge

---

### Workspace（工作区）

软链接目录：

```
workspace/
└── proj-a -> /real/path
```

作为统一的入口点。

---

### Knowledge（知识库）

按项目存储：

```
knowledge/<project_id>/
├── memory/         # 手动维护的知识
├── rules/          # 项目规则
├── commits/        # 从 git 历史自动提取
├── conversations/  # 从 Claude Code 对话自动提取
└── live/           # 通过 MCP 工具实时捕获
```

知识通过 Qdrant 向量数据库进行语义搜索。每个项目对应一个 collection：`ph-knowledge-{project_id}`。

---

### Todo（待办）

存储在 SQLite 中：

- 关联 project_id
- 作为 Agent 上下文使用

---

### Agent（代理）

定义于：

```
agents/<name>.toml
```

包含：

- model
- system_prompt

阶段特定 Agent 由 `service::resolve_agent_for_stage()` 自动解析。

### Work Item（工作项）

一个逻辑上的 todo，可能跨越多个项目。`ph todo list` 和 `ph work` 中按 `title + priority + created_at` 分组展示。

### Todo Documents（工作流文档）

每个 todo 都有专属的文档目录：

```
docs/<todo-id>/
├── 01-requirements.md
├── 02-design.md
├── 03-tasks.md
├── 04-progress.md
└── 05-review.md
```

这些文档驱动 `ph work` 的阶段检测，并作为工作流检查点。

---

## 🤖 Agent 执行模型

### `ph run <project>`

执行步骤：

1. 从 `projects.toml` 加载项目
2. 加载 Agent 配置
3. 加载待办 todo
4. 加载知识库文件
5. 构建 prompt
6. 调用 Claude API
7. 输出结果

### `ph work` — 多阶段 Todo 工作流

`ph work` 启动一个交互式、分阶段的 todo 处理流程。每个 todo 以 `title + priority + created_at` 标识，可跨多个项目。

阶段定义（根据已有文档自动恢复）：

| 阶段 | Agent | 输出文档 |
|------|-------|---------|
| 1. 需求澄清 | `analyst` | `docs/<todo-id>/01-requirements.md` |
| 2. 方案设计 | `architect` | `docs/<todo-id>/02-design.md` |
| 3. 任务拆解 | `planner` | `docs/<todo-id>/03-tasks.md` |
| 4. 编码实现 | `coder` | `docs/<todo-id>/04-progress.md` + git commits |
| 5. 验收回顾 | `reviewer` | `docs/<todo-id>/05-review.md` |

核心行为：

- 使用自定义 `ratatui` 选择器进行 todo 选择（`j`/`k` 移动，`/` 搜索，`q` 退出）
- 第 4 阶段在 `.worktrees/<todo-id>/` 创建 git worktree 以隔离编码环境
- 每阶段开始前要求确认，除非传入 `--yes` 参数
- 工作流中明确禁止 `git push`
- 流程结束时可将 todo 标记为完成

### 交互式选择器（`ph work`）

选择器基于 `ratatui` + `crossterm` 构建，支持：

- **Normal 模式**：`j`/`k` 或 `↑`/`↓` 移动，`/` 进入搜索，`Enter` 选择，`q` 退出
- **Search 模式**：输入内容进行 `SkimMatcherV2` 模糊过滤，`Esc` 清空，`Enter` 确认
- 卡片固定 92 字符宽，垂直居中，主界面无边框，顶部为 ASCII logo
- 详情面板显示翻译后的阶段名称和项目信息，支持 CJK 对齐
- `c` 键复制选中 todo 标题到剪贴板

---

### MCP 服务器（`ph mcp-server`）

通过 MCP 协议（JSON-RPC over stdio）暴露工具给 Claude Code：

| 工具 | 功能 |
|------|------|
| `ph_knowledge_search` | 语义搜索项目知识库 |
| `ph_todo_list` | 列出待办事项 |
| `ph_todo_doc_read` | 读取 todo 阶段文档 |
| `ph_knowledge_read` | 读取知识库文件 |
| `ph_knowledge_capture` | 实时捕获知识到向量库 |

配置位置：`~/.claude.json` 的 `mcpServers.ph`

### 知识同步（`ph knowledge sync`）

从 git commits 和 Claude Code 对话中自动提取知识：

```bash
ph knowledge sync --project <id>   # 同步指定项目
ph knowledge sync                  # 同步所有项目
```

`ph work` 工作流中每阶段结束后自动触发 git 知识同步。

---

## 🧠 Prompt 构建规则

Prompt 必须包含：

- System prompt（来自 Agent）
- 项目信息（name、path）
- 待办事项（仅 pending）
- 知识库（文件内容）
- 用户输入（可选）

避免：

- 转储整个文件系统
- 超出 token 限制

---

## ⚠️ 约束

### 禁止：

- 绕过 service 层
- 混用存储类型（例如在 SQLite 和 TOML 中重复存储数据）
- 引入全局可变状态
- 硬编码绝对路径
- 盲目将整个大文件加载进 prompt
- 在 `ph work` 工作流中执行 `git push`

---

### 必须：

- 所有路径使用 `data_dir()`
- 保持函数短小、可组合
- 返回 `Result<T>` 并做好错误处理
- 优先选择显式而非隐式行为
- 创建 worktree 前确认 `.worktrees/` 已被 git ignore

---

## 🛠 编码规范

### Rust

- 使用 `anyhow::Result`
- 生产代码避免 unwrap
- 保持模块职责单一

---

### 异步

- 仅在必要时使用 async（数据库、HTTP）
- 不要过度 async

---

### 文件 IO

- 必须通过 `infra` 层
- 禁止直接在 CLI 中操作文件

---

## 🚀 未来扩展

Claude 可帮助实现：

- Web UI（axum + 嵌入式前端）
- 流式 LLM 响应
- 使用工具的 Agent
- 后台 / 守护进程模式
- 基于知识图谱或语义搜索的文档检索

---

## 🧪 修改代码时

修改前：

1. 确定正确的层级
2. 检查逻辑是否已存在
3. 避免重复
4. 保持现有架构不变

---

## 🧠 心智模型

将本项目视为：

> 一个**面向开发项目的本地 AI 操作系统**

而不是：

- Web 应用后端
- 微服务系统
- 单体 SaaS

---

## ✅ 推荐的贡献方向

- 短小、可组合的函数
- 清晰的关注点分离
- 改进开发者工作流
- 更好的 Agent 上下文构建

---

## ❌ 反模式

- 神函数（God functions）
- CLI 与业务逻辑混合
- CLI 直接访问数据库
- 硬编码配置路径

---

## 📌 总结

本项目是：

- 本地优先
- 单一二进制
- 结构清晰
- 可扩展

请始终尊重架构设计。
