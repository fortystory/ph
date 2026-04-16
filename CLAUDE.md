# Project Hub (ph) — Claude Guide

## 🧭 Project Overview

This is a **local-first AI project management hub** written in Rust.

The system is built as:

- A **single binary (`ph`)**
- Operating on a **data directory (~/.project-hub)**
- Providing:
  - CLI interface
  - (future) Web UI
  - AI Agent execution

---

## 🧱 Architecture Principles

### 1. Single Binary

All functionality must remain inside ONE executable:

- CLI
- Web server
- Agent runtime

DO NOT split into multiple services.

---

### 2. External State (Data Dir)

All runtime data is stored outside the binary:

```
~/.project-hub/
```

Includes:

- `projects.toml`
- `agents/`
- `prompts/`
- `knowledge/`
- `workspace/` (symlinks)
- `todos.db`
- `docs/` (todo workflow documents)

⚠️ Never hardcode data inside the binary.

---

### 3. Storage Rules

| Type        | Storage |
|------------|--------|
| Projects   | TOML   |
| Agents     | TOML   |
| Prompts    | Files  |
| Todos      | SQLite |
| Workspace  | Symlink |
| Docs       | Markdown |

---

### 4. Layered Design

Strict layering must be followed:

```
CLI / Web
↓
Service Layer
↓
Infra Layer
↓
Storage (FS / SQLite)
```

❗ Rules:

- CLI must NOT access infra directly
- Business logic must live in `service`
- `core` contains only data structures

---

## 📁 Repository Structure

```
crates/
├── core/       # data models only
├── infra/      # file system, sqlite, symlink
├── service/    # business logic
├── cli/        # command line interface
└── web/        # http server (future)
```

---

## 🧩 Key Concepts

### Project

Represents a managed codebase:

- id
- name
- path
- agent
- prompt
- knowledge

---

### Workspace

A directory of symlinks:

```
workspace/
└── proj-a -> /real/path
```

Used as a unified entry point.

---

### Knowledge

Stored per project:

```
knowledge/<project_id>/
```

Contains markdown or text files.

---

### Todo

Stored in SQLite:

- linked to project_id
- used as agent context

---

### Agent

Defined in:

```
agents/<name>.toml
```

Includes:

- model
- system_prompt

Stage-specific agents are automatically resolved by `service::resolve_agent_for_stage()`.

### Work Item

A logical todo that may span multiple projects. Todos are grouped by `title + priority + created_at` for display in `ph todo list` and `ph work`.

### Todo Documents

Each todo has a dedicated docs directory:

```
docs/<todo-id>/
├── 01-requirements.md
├── 02-design.md
├── 03-tasks.md
├── 04-progress.md
└── 05-review.md
```

These drive the `ph work` stage detection and serve as workflow checkpoints.

---

## 🤖 Agent Execution Model

### `ph run <project>`

Execution steps:

1. Load project from `projects.toml`
2. Load agent config
3. Load todos (not done)
4. Load knowledge files
5. Build prompt
6. Call Claude API
7. Print result

### `ph work` — Multi-Stage Todo Workflow

`ph work` launches an interactive, stage-based workflow for processing todos. Each todo is identified by `title + priority + created_at` and can span multiple projects.

Stages (auto-resumed based on existing docs):

| Stage | Agent | Output Document |
|-------|-------|-----------------|
| 1. 需求澄清 | `analyst` | `docs/<todo-id>/01-requirements.md` |
| 2. 方案设计 | `architect` | `docs/<todo-id>/02-design.md` |
| 3. 任务拆解 | `planner` | `docs/<todo-id>/03-tasks.md` |
| 4. 编码实现 | `coder` | `docs/<todo-id>/04-progress.md` + git commits |
| 5. 验收回顾 | `reviewer` | `docs/<todo-id>/05-review.md` |

Key behaviors:

- Uses a custom `ratatui` picker for todo selection (j/k navigate, `/` search, `q` quit)
- Stage 4 creates a git worktree at `.worktrees/<todo-id>/` for isolated coding
- Each stage prompts for confirmation unless `--yes` is passed
- `git push` is explicitly forbidden in the workflow
- Completed todos can be marked done at the end

### Interactive Picker (`ph work`)

The picker is built with `ratatui` + `crossterm` and supports:

- **Normal mode**: `j`/`k` or `↑`/`↓` to move, `/` to enter search, `Enter` to select, `q` to quit
- **Search mode**: type to fuzzy-filter with `SkimMatcherV2`, `Esc` to clear, `Enter` to confirm
- Card is fixed 92 chars wide, vertically centered, borderless main UI with an ASCII logo header
- Details pane shows translated stage names and project info with CJK-aware alignment

---

## 🧠 Prompt Construction Rules

Prompt must include:

- System prompt (from agent)
- Project info (name, path)
- Todos (pending only)
- Knowledge (file contents)
- User input (optional)

Avoid:

- dumping entire filesystem
- exceeding token limits

---

## ⚠️ Constraints

### DO NOT:

- Bypass service layer
- Mix storage types (e.g., duplicating data in SQLite + TOML)
- Introduce global mutable state
- Hardcode absolute paths
- Load entire large files blindly into prompt
- `git push` from within `ph work` workflow

---

### ALWAYS:

- Use `data_dir()` for all paths
- Keep functions small and composable
- Return `Result<T>` with proper error handling
- Prefer explicit over implicit behavior
- Verify `.worktrees/` is ignored before creating worktrees

---

## 🛠 Coding Guidelines

### Rust

- Use `anyhow::Result`
- Avoid unwrap in production code
- Keep modules focused

---

### Async

- Use async ONLY when necessary (DB, HTTP)
- Do not over-async everything

---

### File IO

- Must go through `infra`
- Never directly in CLI

---

## 🚀 Future Extensions

Claude may help implement:

- Web UI (axum + embedded frontend)
- Streaming LLM responses
- Tool-using agents
- Background / daemon mode for agents
- Knowledge graph or semantic search over docs

---

## 🧪 When Modifying Code

Before making changes:

1. Identify the correct layer
2. Check if logic already exists
3. Avoid duplication
4. Preserve existing architecture

---

## 🧠 Mental Model

Think of this project as:

> A **local AI operating system for development projects**

NOT:

- a web app backend
- a microservice system
- a monolithic SaaS

---

## ✅ Preferred Contributions

- Small, composable functions
- Clear separation of concerns
- Improvements to developer workflow
- Better agent context construction

---

## ❌ Anti-Patterns

- God functions
- Mixing CLI + business logic
- Direct DB access from CLI
- Hardcoded config paths

---

## 📌 Summary

This project is:

- local-first
- single-binary
- structured
- extensible

Respect the architecture at all times.
