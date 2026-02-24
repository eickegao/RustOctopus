use anyhow::Result;

use nanobot_core::config::factory::resolve_workspace_path;
use nanobot_core::config::loader::{default_config_path, load_config};

pub fn run() -> Result<()> {
    let config_path = default_config_path();
    let config = load_config(None)?;

    let workspace = resolve_workspace_path(&config.agents.defaults.workspace);

    println!("nanobot status");
    println!("==============");

    // Config
    println!(
        "\nConfig:    {} ({})",
        config_path.display(),
        if config_path.exists() { "found" } else { "not found" }
    );

    // Workspace
    println!(
        "Workspace: {} ({})",
        workspace.display(),
        if workspace.exists() { "found" } else { "not found" }
    );

    // Model
    println!("Model:     {}", config.agents.defaults.model);

    // Agent settings
    println!("\nAgent Settings:");
    println!("  Max tokens:     {}", config.agents.defaults.max_tokens);
    println!("  Temperature:    {}", config.agents.defaults.temperature);
    println!("  Max iterations: {}", config.agents.defaults.max_tool_iterations);
    println!("  Memory window:  {}", config.agents.defaults.memory_window);

    // Provider API keys
    println!("\nProviders:");
    let providers: &[(&str, &str)] = &[
        ("Anthropic", &config.providers.anthropic.api_key),
        ("OpenAI", &config.providers.openai.api_key),
        ("OpenRouter", &config.providers.openrouter.api_key),
        ("DeepSeek", &config.providers.deepseek.api_key),
        ("Groq", &config.providers.groq.api_key),
        ("Gemini", &config.providers.gemini.api_key),
        ("Moonshot", &config.providers.moonshot.api_key),
        ("Zhipu", &config.providers.zhipu.api_key),
        ("DashScope", &config.providers.dashscope.api_key),
        ("MiniMax", &config.providers.minimax.api_key),
        ("vLLM", &config.providers.vllm.api_key),
        ("AiHubMix", &config.providers.aihubmix.api_key),
        ("SiliconFlow", &config.providers.siliconflow.api_key),
        ("VolcEngine", &config.providers.volcengine.api_key),
    ];

    for (name, key) in providers {
        let status = if key.is_empty() {
            "not set"
        } else {
            "set"
        };
        println!("  {:<12} {}", format!("{}:", name), status);
    }

    // Channels
    println!("\nChannels:");
    println!(
        "  Telegram:  {}",
        if config.channels.telegram.enabled { "enabled" } else { "disabled" }
    );
    println!(
        "  Feishu:    {}",
        if config.channels.feishu.enabled { "enabled" } else { "disabled" }
    );

    // Gateway
    println!("\nGateway:   {}:{}", config.gateway.host, config.gateway.port);

    Ok(())
}
