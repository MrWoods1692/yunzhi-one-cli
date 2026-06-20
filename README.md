# yunzhi-one-cli

一个命令，连接所有智能能力。全能 AI 智能体命令行平台。

## 云智 One CLI

`yunzhi` 是一个终端内对话式智能体工具，MVP 采用纯 stdout 流式渲染，核心架构已按后续 ratatui 全屏 TUI 预留：LLM 客户端、Agent Loop、工具系统和权限确认彼此解耦。

## 快速开始

```bash
cargo run -- config set-key sk-xxxx
cargo run
```

也可以首次直接启动：

```bash
cargo run
```

未配置 API Key 时会自动进入引导并保存到 `~/.yunzhi/config.toml`：

```toml
api_key = "sk-xxxx"
```

## 命令

```bash
cargo run -- config show
cargo run -- config set-key sk-xxxx
cargo run -- -p "阅读 README 并总结项目"
cargo run -- print "列出当前目录文件"
cargo run -- --dangerously-skip-permissions -p "运行 cargo test"
```

安装后启动命令为：

```bash
cargo install --path .
yunzhi
```

## MVP 能力

- 兼容 Anthropic Messages API 风格，请求固定发送到 `https://yunzhiapi.cn/v1/messages`。
- 支持 `stream: true` SSE 流式响应解析。
- 预留 `LlmClient` trait，真实接口格式变化时可替换适配层。
- 支持 `read_file`、`write_file`、`edit_file`、`bash`、`glob_search`、`grep_search`、`list_dir`。
- 写文件、编辑文件、执行 bash 默认需要确认，支持 `--dangerously-skip-permissions` 跳过。
- 启动时读取项目级 `.yunzhi/memory.md` 并注入 system prompt。
- 对话历史保存在内存中，超过阈值后做简单摘要压缩。
- 交互模式支持 `/help`、`/clear`、`/exit`。

## 设计取舍

当前版本优先交付可通过 `cargo run` 使用的 Agent 核心闭环，因此 UI 先采用 stdout 打字机效果和 ANSI 颜色。`ratatui` 与 `crossterm` 已作为依赖引入，后续可以在不改 Agent Loop 的前提下替换为固定输入框、滚动输出区和状态栏的全屏 TUI。