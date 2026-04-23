use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::io::Write;

use crate::{embed::Embedder, knowledge_index, todo};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

pub async fn run_mcp_server() -> Result<()> {
    let pool = crate::init_db().await?;
    let stdin = tokio::io::stdin();
    let reader = tokio::io::BufReader::new(stdin);
    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                send_response(JsonRpcResponse::error(None, -32700, format!("Parse error: {e}")));
                continue;
            }
        };

        let response = handle_request(&pool, request).await;
        send_response(response);
    }

    Ok(())
}

fn send_response(response: JsonRpcResponse) {
    let json = serde_json::to_string(&response).unwrap();
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{json}");
    let _ = stdout.flush();
}

async fn handle_request(pool: &SqlitePool, req: JsonRpcRequest) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => handle_initialize(req.id),
        "initialized" => JsonRpcResponse::success(req.id, json!({})),
        "tools/list" => handle_tools_list(req.id),
        "tools/call" => handle_tools_call(pool, req.id, req.params).await,
        _ => JsonRpcResponse::error(req.id, -32601, format!("Method not found: {}", req.method)),
    }
}

fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "ph",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
    )
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        json!({
            "tools": [
                {
                    "name": "ph_knowledge_search",
                    "description": "语义搜索项目知识库，返回与查询最相关的文本片段",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "project_id": {
                                "type": "string",
                                "description": "项目 ID"
                            },
                            "query": {
                                "type": "string",
                                "description": "搜索查询"
                            },
                            "top_k": {
                                "type": "integer",
                                "description": "返回结果数量",
                                "default": 5
                            }
                        },
                        "required": ["project_id", "query"]
                    }
                },
                {
                    "name": "ph_todo_list",
                    "description": "列出 ph 中管理的待办事项",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "project_id": {
                                "type": "string",
                                "description": "按项目过滤"
                            },
                            "status": {
                                "type": "string",
                                "description": "按状态过滤，如 pending 或 done"
                            }
                        }
                    }
                },
                {
                    "name": "ph_todo_doc_read",
                    "description": "读取某个 todo 的阶段工作流文档",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "todo_id": {
                                "type": "string",
                                "description": "Todo ID 或短 ID"
                            },
                            "stage": {
                                "type": "string",
                                "description": "阶段名称: requirements, design, tasks, progress, review"
                            }
                        },
                        "required": ["todo_id", "stage"]
                    }
                },
                {
                    "name": "ph_knowledge_read",
                    "description": "直接读取项目知识库中的指定文件内容",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "project_id": {
                                "type": "string",
                                "description": "项目 ID"
                            },
                            "file_path": {
                                "type": "string",
                                "description": "知识库中的相对文件路径"
                            }
                        },
                        "required": ["project_id", "file_path"]
                    }
                }
            ]
        }),
    )
}

async fn handle_tools_call(
    pool: &SqlitePool,
    id: Option<Value>,
    params: Option<Value>,
) -> JsonRpcResponse {
    let params = match params {
        Some(Value::Object(p)) => p,
        _ => {
            return JsonRpcResponse::error(id, -32602, "Invalid params".to_string());
        }
    };

    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match name {
        "ph_knowledge_search" => match handle_knowledge_search(pool, arguments).await {
            Ok(result) => JsonRpcResponse::success(id, result),
            Err(e) => JsonRpcResponse::error(id, -32000, format!("Knowledge search failed: {e}")),
        },
        "ph_todo_list" => match handle_todo_list(pool, arguments).await {
            Ok(result) => JsonRpcResponse::success(id, result),
            Err(e) => JsonRpcResponse::error(id, -32000, format!("Todo list failed: {e}")),
        },
        "ph_todo_doc_read" => match handle_todo_doc_read(arguments) {
            Ok(result) => JsonRpcResponse::success(id, result),
            Err(e) => JsonRpcResponse::error(id, -32000, format!("Doc read failed: {e}")),
        },
        "ph_knowledge_read" => match handle_knowledge_read(arguments) {
            Ok(result) => JsonRpcResponse::success(id, result),
            Err(e) => JsonRpcResponse::error(id, -32000, format!("Knowledge read failed: {e}")),
        },
        _ => JsonRpcResponse::error(id, -32601, format!("Tool not found: {name}")),
    }
}

async fn handle_knowledge_search(pool: &SqlitePool, args: Value) -> Result<Value> {
    let project_id = args["project_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("project_id required"))?;
    let query = args["query"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("query required"))?;
    let top_k = args["top_k"].as_i64().unwrap_or(5);

    let embedder = Embedder::new()?;

    if knowledge_index::is_stale(pool, project_id).await? {
        knowledge_index::update_index(pool, project_id, &embedder).await?;
    }

    let query_vec = embedder.embed_query(query)?;
    let chunks = knowledge_index::search(pool, project_id, query_vec, top_k).await?;

    let results: Vec<Value> = chunks
        .into_iter()
        .map(|c| {
            json!({
                "file_path": c.file_path,
                "chunk_index": c.chunk_index,
                "content": c.content,
                "updated_at": c.updated_at,
            })
        })
        .collect();

    Ok(json!({ "results": results }))
}

async fn handle_todo_list(pool: &SqlitePool, args: Value) -> Result<Value> {
    let project_id = args["project_id"].as_str();
    let status_filter = args["status"].as_str();

    let todos = todo::list_todos(pool, project_id).await?;

    let items: Vec<Value> = todos
        .into_iter()
        .filter(|t| {
            if let Some(filter) = status_filter {
                t.status == filter
            } else {
                true
            }
        })
        .map(|t| {
            json!({
                "id": t.id,
                "project_id": t.project_id,
                "title": t.title,
                "status": t.status,
                "priority": t.priority,
                "created_at": t.created_at,
                "completed_at": t.completed_at,
            })
        })
        .collect();

    Ok(json!({ "todos": items }))
}

fn handle_todo_doc_read(args: Value) -> Result<Value> {
    let todo_id = args["todo_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("todo_id required"))?;
    let stage = args["stage"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("stage required"))?;

    let stage_num = match stage {
        "requirements" => "01",
        "design" => "02",
        "tasks" => "03",
        "progress" => "04",
        "review" => "05",
        _ => stage,
    };

    let dir = crate::todo_docs_dir(todo_id);
    let path = dir.join(format!("{}-{}.md", stage_num, stage));

    let content = if path.exists() {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };

    Ok(json!({
        "todo_id": todo_id,
        "stage": stage,
        "content": content,
        "exists": !content.is_empty(),
    }))
}

fn handle_knowledge_read(args: Value) -> Result<Value> {
    let project_id = args["project_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("project_id required"))?;
    let file_path = args["file_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("file_path required"))?;

    let base = crate::knowledge_dir().join(project_id);
    let path = base.join(file_path);

    if !path.exists() {
        anyhow::bail!("file not found: {}", file_path);
    }

    let content = std::fs::read_to_string(&path)?;

    Ok(json!({
        "project_id": project_id,
        "file_path": file_path,
        "content": content,
    }))
}
