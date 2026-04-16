# Project Hub (ph)

一个本地优先的 AI 项目管理工具。

## 简介

`ph` 是一个用 Rust 编写的单二进制 CLI 工具，帮助开发者通过 AI Agent 以结构化的方式管理项目、todo 和处理编码任务。

所有运行时数据存储在 `~/.project-hub/` 中，采用 local-first 设计，不依赖远程服务。

## 特性

- **项目管理**：注册项目路径，通过统一的 `workspace/` 软链接入口访问
- **Todo 管理**：支持跨项目的 todo，使用 SQLite 持久化
- **AI Agent 执行**：`ph run <project>` 一键加载项目上下文并调用 Claude API
- **结构化工作流**：`ph work` 提供五阶段交互式 todo 处理流程
  1. 需求澄清 (`analyst`)
  2. 方案设计 (`architect`)
  3. 任务拆解 (`planner`)
  4. 编码实现 (`coder`) — 使用 git worktree 隔离
  5. 验收回顾 (`reviewer`)
- **自定义 TUI 选择器**：内置 `ratatui` 交互界面，支持 j/k 导航、`/` 模糊搜索

## 架构

```
crates/
├── core/       # 数据模型
├── infra/      # 文件系统、SQLite、软链接、git worktree
├── service/    # 业务逻辑
├── cli/        # 命令行界面
└── web/        # HTTP 服务（未来）
```

严格遵守分层设计：CLI 不直接访问 infra，业务逻辑统一放在 `service` 层。

## 快速开始

```bash
# 初始化数据目录
ph init

# 添加项目
ph project add myapp "My App" /path/to/myapp

# 添加 todo
ph todo add --project myapp "实现用户登录"

# 查看 todo 列表
ph todo list

# 启动结构化工作流
ph work

# 直接运行 Agent
ph run myapp "帮我review最近的改动"
```

## 数据目录

```
~/.project-hub/
├── projects.toml     # 项目配置
├── agents/           # Agent TOML 配置
├── prompts/          # 提示词模板
├── knowledge/        # 项目知识库
├── workspace/        # 项目软链接
├── docs/             # Todo 工作流文档
└── todos.db          # SQLite 数据库
```

## 许可证

MIT
