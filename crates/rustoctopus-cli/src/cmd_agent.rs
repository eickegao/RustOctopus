use anyhow::Result;
use rustoctopus_core::agent::agent_loop::AgentLoop;
use rustoctopus_core::bus::queue::MessageBus;
use rustoctopus_core::config::factory::create_provider;
use rustoctopus_core::config::schema::Config;

/// Run a single message through the agent and print the response.
pub async fn run_single(message: &str, session_id: &str, config: Config) -> Result<()> {
    let provider = create_provider(&config)?;
    let (bus, inbound_rx, _outbound_rx) = MessageBus::new();
    let mut agent = AgentLoop::from_config(config, bus, provider, inbound_rx);
    let response = agent.process_direct(message, session_id).await?;
    println!("{}", response);
    Ok(())
}

/// Run the agent in interactive REPL mode with rustyline.
pub async fn run_interactive(session_id: &str, config: Config) -> Result<()> {
    println!("RustOctopus interactive mode. Type 'exit' to quit.\n");

    let provider = create_provider(&config)?;
    let (bus, inbound_rx, _outbound_rx) = MessageBus::new();
    let mut agent = AgentLoop::from_config(config, bus, provider, inbound_rx);

    let history_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".rustoctopus")
        .join("history.txt");

    let mut rl = rustyline::DefaultEditor::new()?;
    let _ = rl.load_history(&history_path);

    loop {
        match rl.readline("You: ") {
            Ok(line) => {
                let input = line.trim();
                if matches!(input, "exit" | "quit" | "/exit" | "/quit" | ":q") {
                    break;
                }
                if input.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(input);
                match agent.process_direct(input, session_id).await {
                    Ok(response) => {
                        println!("\nAssistant: {}\n", response);
                    }
                    Err(e) => {
                        eprintln!("\nError: {}\n", e);
                    }
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted | rustyline::error::ReadlineError::Eof) => {
                break;
            }
            Err(e) => {
                eprintln!("Input error: {}", e);
                break;
            }
        }
    }

    let _ = rl.save_history(&history_path);
    println!("Goodbye!");
    Ok(())
}
