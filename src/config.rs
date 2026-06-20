use crate::types::{AgentMode, AppConfig};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const MEMORY_ENTRY_PREFIX: &str = "- [id:";
const MEMORY_PROMPT_LIMIT: usize = 40;

pub fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("无法定位用户 home 目录")?;
    Ok(home.join(".yunzhi"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn memory_path() -> Result<PathBuf> {
    Ok(std::env::current_dir()?.join(".yunzhi").join("memory.md"))
}

pub fn global_profiles_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("profiles.toml"))
}

pub fn project_profiles_path(cwd: &Path) -> PathBuf {
    cwd.join(".yunzhi").join("profiles.toml")
}

pub fn load_config() -> Result<Option<AppConfig>> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("读取配置失败: {}", path.display()))?;
    let cfg = toml::from_str::<AppConfig>(&raw).context("解析 ~/.yunzhi/config.toml 失败")?;
    if cfg.api_key.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(cfg))
    }
}

pub fn save_config(config: &AppConfig) -> Result<()> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir).with_context(|| format!("创建配置目录失败: {}", dir.display()))?;
    let raw = toml::to_string_pretty(config).context("序列化配置失败")?;
    let path = config_path()?;
    fs::write(&path, raw).with_context(|| format!("写入配置失败: {}", path.display()))?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProfilesFile {
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProfileConfig {
    pub persona: Option<String>,
    pub mode: Option<AgentMode>,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub tools: Option<Vec<String>>,
}

pub fn load_profile(cwd: &Path, name: &str) -> Result<Option<ProfileConfig>> {
    let project_path = project_profiles_path(cwd);
    if let Some(profile) =
        read_profiles_file(&project_path)?.and_then(|file| file.profiles.get(name).cloned())
    {
        return Ok(Some(profile));
    }
    let global_path = global_profiles_path()?;
    Ok(read_profiles_file(&global_path)?.and_then(|file| file.profiles.get(name).cloned()))
}

fn read_profiles_file(path: &Path) -> Result<Option<ProfilesFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("读取 profile 配置失败: {}", path.display()))?;
    let profiles = toml::from_str::<ProfilesFile>(&raw)
        .with_context(|| format!("解析 profile 配置失败: {}", path.display()))?;
    Ok(Some(profiles))
}

pub fn ensure_config_interactive() -> Result<AppConfig> {
    if let Some(config) = load_config()? {
        return Ok(config);
    }

    println!("首次运行需要配置云智 One API Key。");
    print!("请输入 API Key: ");
    io::stdout().flush()?;
    let mut api_key = String::new();
    io::stdin().read_line(&mut api_key)?;
    let api_key = api_key.trim().to_string();
    anyhow::ensure!(!api_key.is_empty(), "API Key 不能为空");
    let config = AppConfig {
        api_key,
        model: Some(crate::llm::DEFAULT_MODEL.to_string()),
    };
    save_config(&config)?;
    println!("已保存到 {}", config_path()?.display());
    Ok(config)
}

pub fn masked_key(api_key: &str) -> String {
    let chars: Vec<char> = api_key.chars().collect();
    if chars.len() <= 8 {
        return "****".to_string();
    }
    let prefix: String = chars.iter().take(4).collect();
    let suffix: String = chars
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}****{}", prefix, suffix)
}

pub fn load_project_memory() -> Result<Option<String>> {
    let path = memory_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("读取项目记忆失败: {}", path.display()))?;
    if content.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(content))
    }
}

pub fn load_project_memory_prompt() -> Result<Option<String>> {
    Ok(load_project_memory()?.map(|content| render_memory_prompt(&content)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryEntry {
    pub id: String,
    pub tags: Vec<String>,
    pub text: String,
}

pub fn parse_memory_entries(content: &str) -> Vec<MemoryEntry> {
    content
        .lines()
        .filter_map(parse_memory_entry_line)
        .collect()
}

pub fn render_memory_entry(entry: &MemoryEntry) -> String {
    let tags = if entry.tags.is_empty() {
        String::new()
    } else {
        format!(" [tags:{}]", entry.tags.join(","))
    };
    format!("- [id:{}]{} {}", entry.id, tags, entry.text.trim())
}

pub fn render_memory_prompt(content: &str) -> String {
    let entries = parse_memory_entries(content);
    if entries.is_empty() {
        return content.trim().to_string();
    }
    let mut rendered = entries
        .iter()
        .rev()
        .take(MEMORY_PROMPT_LIMIT)
        .rev()
        .map(render_memory_entry)
        .collect::<Vec<_>>()
        .join("\n");
    let omitted = entries.len().saturating_sub(MEMORY_PROMPT_LIMIT);
    if omitted > 0 {
        rendered.push_str(&format!(
            "\n... 已省略 {omitted} 条较早记忆，可用 long_memory search 查询。"
        ));
    }
    rendered
}

pub fn search_memory_entries<'a>(entries: &'a [MemoryEntry], query: &str) -> Vec<&'a MemoryEntry> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return entries.iter().collect();
    }
    let mut scored = entries
        .iter()
        .filter_map(|entry| {
            let score = memory_entry_score(entry, &terms);
            (score > 0).then_some((score, entry))
        })
        .collect::<Vec<_>>();
    scored.sort_by_key(|(score, entry)| (Reverse(*score), Reverse(entry.id.clone())));
    scored.into_iter().map(|(_, entry)| entry).collect()
}

fn parse_memory_entry_line(line: &str) -> Option<MemoryEntry> {
    let rest = line.trim().strip_prefix(MEMORY_ENTRY_PREFIX)?;
    let (id, rest) = rest.split_once(']')?;
    let rest = rest.trim_start();
    let (tags, text) = if let Some(rest) = rest.strip_prefix("[tags:") {
        let (raw_tags, text) = rest.split_once(']')?;
        (
            raw_tags
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            text.trim_start(),
        )
    } else {
        (Vec::new(), rest)
    };
    let id = id.trim().to_string();
    let text = text.trim().to_string();
    (!id.is_empty() && !text.is_empty()).then_some(MemoryEntry { id, tags, text })
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ';')
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(|term| term.to_lowercase())
        .collect()
}

fn memory_entry_score(entry: &MemoryEntry, terms: &[String]) -> usize {
    let text = entry.text.to_lowercase();
    let id = entry.id.to_lowercase();
    let tags = entry
        .tags
        .iter()
        .map(|tag| tag.to_lowercase())
        .collect::<Vec<_>>();
    terms
        .iter()
        .map(|term| {
            let id_score = (id == *term) as usize * 8;
            let tag_score = tags
                .iter()
                .filter(|tag| tag.contains(term.as_str()))
                .count()
                * 4;
            let text_score = text.matches(term.as_str()).count();
            id_score + tag_score + text_score
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_key() {
        assert_eq!(masked_key("sk-1234567890"), "sk-1****7890");
        assert_eq!(masked_key("short"), "****");
    }

    #[test]
    fn loads_config_without_model() {
        let config = toml::from_str::<AppConfig>("api_key = \"sk-test\"\n").unwrap();
        assert_eq!(config.api_key, "sk-test");
        assert_eq!(config.model, None);
    }

    #[test]
    fn loads_project_profile() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".yunzhi")).unwrap();
        std::fs::write(
            dir.path().join(".yunzhi/profiles.toml"),
            "[profiles.rust]\npersona = \"Rust reviewer\"\nmode = \"agent\"\nmodel = \"custom-model\"\nmax_tokens = 2048\ntools = [\"read_file\", \"test_loop\"]\n",
        )
        .unwrap();

        let profile = load_profile(dir.path(), "rust").unwrap().unwrap();
        assert_eq!(profile.persona.as_deref(), Some("Rust reviewer"));
        assert_eq!(profile.mode, Some(AgentMode::Agent));
        assert_eq!(profile.model.as_deref(), Some("custom-model"));
        assert_eq!(profile.max_tokens, Some(2048));
        assert_eq!(profile.tools.unwrap(), vec!["read_file", "test_loop"]);
    }
}
