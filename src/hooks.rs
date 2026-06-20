use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct HookConfig {
    #[serde(default)]
    pub hooks: Vec<HookRule>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct HookRule {
    pub event: HookEvent,
    #[serde(default)]
    pub tools: Vec<String>,
    pub command: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    PreTool,
    PostTool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRun {
    pub command: String,
    pub status: String,
    pub stdout: String,
    pub stderr: String,
    pub elapsed_ms: u128,
}

fn default_timeout_secs() -> u64 {
    30
}

pub async fn run_matching_hooks(
    cwd: &Path,
    event: HookEvent,
    tool_name: &str,
    tool_input: &Value,
    tool_output: Option<&str>,
) -> Result<Vec<HookRun>> {
    let config = match load_hook_config(cwd)? {
        Some(config) => config,
        None => return Ok(Vec::new()),
    };
    let mut runs = Vec::new();
    for hook in config.hooks {
        if hook.event != event || !hook_matches_tool(&hook, tool_name) {
            continue;
        }
        runs.push(run_hook(cwd, &hook, event, tool_name, tool_input, tool_output).await?);
    }
    Ok(runs)
}

pub fn load_hook_config(cwd: &Path) -> Result<Option<HookConfig>> {
    let path = cwd.join(".yunzhi/hooks.toml");
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("读取 Hook 配置失败: {}", path.display()))?;
    let config = toml::from_str::<HookConfig>(&raw)
        .with_context(|| format!("解析 Hook 配置失败: {}", path.display()))?;
    Ok(Some(config))
}

pub fn format_hook_runs(runs: &[HookRun]) -> String {
    if runs.is_empty() {
        return String::new();
    }
    let mut rendered = String::from("hooks:\n");
    for run in runs {
        rendered.push_str(&format!(
            "- command: {}\n  status: {}\n  elapsed_ms: {}\n",
            run.command, run.status, run.elapsed_ms
        ));
        if !run.stdout.trim().is_empty() {
            rendered.push_str("  stdout:\n");
            for line in run.stdout.lines().take(40) {
                rendered.push_str("    ");
                rendered.push_str(line);
                rendered.push('\n');
            }
        }
        if !run.stderr.trim().is_empty() {
            rendered.push_str("  stderr:\n");
            for line in run.stderr.lines().take(40) {
                rendered.push_str("    ");
                rendered.push_str(line);
                rendered.push('\n');
            }
        }
    }
    rendered.trim_end().to_string()
}

fn hook_matches_tool(hook: &HookRule, tool_name: &str) -> bool {
    hook.tools.is_empty()
        || hook
            .tools
            .iter()
            .any(|tool| tool == "*" || tool == tool_name)
}

async fn run_hook(
    cwd: &Path,
    hook: &HookRule,
    event: HookEvent,
    tool_name: &str,
    tool_input: &Value,
    tool_output: Option<&str>,
) -> Result<HookRun> {
    let started = Instant::now();
    let mut process = Command::new("sh");
    process
        .arg("-c")
        .arg(&hook.command)
        .current_dir(cwd)
        .env("YUNZHI_HOOK_EVENT", format!("{:?}", event))
        .env("YUNZHI_TOOL_NAME", tool_name)
        .env("YUNZHI_TOOL_INPUT", tool_input.to_string());
    if let Some(output) = tool_output {
        process.env("YUNZHI_TOOL_OUTPUT", output);
    }
    let output = timeout(
        Duration::from_secs(hook.timeout.clamp(1, 600)),
        process.output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Hook 超时: {}", hook.command))??;
    let run = HookRun {
        command: hook.command.clone(),
        status: output.status.to_string(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        elapsed_ms: started.elapsed().as_millis(),
    };
    if !output.status.success() {
        anyhow::bail!("Hook 执行失败:\n{}", format_hook_runs(&[run]));
    }
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn runs_matching_hook() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".yunzhi")).unwrap();
        std::fs::write(
            dir.path().join(".yunzhi/hooks.toml"),
            r#"[[hooks]]
event = "post_tool"
tools = ["write_file"]
command = "printf $YUNZHI_TOOL_NAME > hook.out"
"#,
        )
        .unwrap();

        let runs = run_matching_hooks(
            dir.path(),
            HookEvent::PostTool,
            "write_file",
            &serde_json::json!({"path":"a.txt"}),
            Some("ok"),
        )
        .await
        .unwrap();

        assert_eq!(runs.len(), 1);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("hook.out")).unwrap(),
            "write_file"
        );
    }
}
