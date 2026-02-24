use std::path::PathBuf;

use anyhow::Result;

use rustoctopus_core::config::loader::{default_config_path, save_config};
use rustoctopus_core::config::schema::Config;

const AGENTS_MD: &str = r#"# Agents

Define your agent personas and behaviors here.
"#;

const SOUL_MD: &str = r#"# Soul

Define the core personality and values of your assistant.
"#;

const USER_MD: &str = r#"# User

Information about the user to help the assistant provide personalized responses.
"#;

const TOOLS_MD: &str = r#"# Tools

Custom tool definitions and documentation.
"#;

const IDENTITY_MD: &str = r#"# Identity

The identity and name of your assistant.

Name: RustOctopus
"#;

pub fn run() -> Result<()> {
    let app_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rustoctopus");

    let config_path = default_config_path();
    let workspace_dir = app_dir.join("workspace");
    let skills_dir = workspace_dir.join("skills");
    let sessions_dir = workspace_dir.join("sessions");

    // Create directories
    std::fs::create_dir_all(&workspace_dir)?;
    std::fs::create_dir_all(&skills_dir)?;
    std::fs::create_dir_all(&sessions_dir)?;

    // Create config.json if missing
    if !config_path.exists() {
        let config = Config::default();
        save_config(&config, Some(&config_path))?;
        println!("Created config: {}", config_path.display());
    } else {
        println!("Config already exists: {}", config_path.display());
    }

    // Create template files if missing
    let templates: &[(&str, &str)] = &[
        ("AGENTS.md", AGENTS_MD),
        ("SOUL.md", SOUL_MD),
        ("USER.md", USER_MD),
        ("TOOLS.md", TOOLS_MD),
        ("IDENTITY.md", IDENTITY_MD),
    ];

    for (name, content) in templates {
        let path = workspace_dir.join(name);
        if !path.exists() {
            std::fs::write(&path, content)?;
            println!("Created: {}", path.display());
        }
    }

    println!("\nRustOctopus setup complete!");
    println!("  Config:    {}", config_path.display());
    println!("  Workspace: {}", workspace_dir.display());
    println!("\nEdit {} to configure your API keys and model.", config_path.display());

    Ok(())
}
