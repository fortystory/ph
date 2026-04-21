use std::path::Path;

pub fn default_stage_instruction(stage: &str, output_dir: &Path) -> String {
    match stage {
        "requirements" => format!(
            "当前阶段：需求澄清。\n\n\
             你的任务是与需求方澄清 todo 的边界条件、验收标准和潜在风险。\n\
             请将澄清后的需求写入 {}/01-requirements.md。\n\
             文档应包含：背景、目标、功能范围、非功能需求、验收标准。\n\
             完成后请退出 Claude Code，以便工作流继续推进。",
            output_dir.display()
        ),
        "design" => format!(
            "当前阶段：方案设计。\n\n\
             请基于 01-requirements.md 设计技术方案。\n\
             将设计文档写入 {}/02-design.md。\n\
             文档应包含：整体架构、数据结构设计、关键接口、风险与回退方案。\n\
             完成后请退出 Claude Code，以便工作流继续推进。",
            output_dir.display()
        ),
        "tasks" => format!(
            "当前阶段：任务拆解。\n\n\
             请基于 01-requirements.md 和 02-design.md 将工作拆解为可执行开发任务。\n\
             将任务列表写入 {}/03-tasks.md。\n\
             每条任务应包含：描述、涉及文件、预估复杂度、依赖关系。\n\
             完成后请退出 Claude Code，以便工作流继续推进。",
            output_dir.display()
        ),
        "progress" => format!(
            "当前阶段：编码实现。\n\n\
             请基于 03-tasks.md 按顺序完成开发任务。\n\
             每个子任务完成后应提交一个 git commit。\n\
             将进度记录写入 {}/04-progress.md。\n\
             完成后请退出 Claude Code，以便工作流继续推进。",
            output_dir.display()
        ),
        "review" => format!(
            "当前阶段：验收回顾。\n\n\
             请对照 01-requirements.md 验收实现是否满足需求。\n\
             将验收结果写入 {}/05-review.md。\n\
             文档应包含：验收项、测试结果、已知问题、后续优化建议。\n\
             完成后请退出 Claude Code，以便工作流继续推进。",
            output_dir.display()
        ),
        _ => format!(
            "当前阶段：{}。\n\n\
             请完成本阶段工作。\n\
             完成后请退出 Claude Code，以便工作流继续推进。",
            stage
        ),
    }
}

pub fn stage_instruction(stage: &str, output_dir: &Path, agent: &core::Agent) -> String {
    if let Some(ref custom) = agent.stage_prompts {
        if let Some(prompt) = custom.get(stage) {
            return prompt.replace("{output_dir}", &output_dir.to_string_lossy());
        }
    }
    default_stage_instruction(stage, output_dir)
}
