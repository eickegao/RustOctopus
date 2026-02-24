use anyhow::Result;
use tracing::info;

use nanobot_core::agent::agent_loop::AgentLoop;
use nanobot_core::bus::queue::MessageBus;
use nanobot_core::channels::{ChannelManager, FeishuChannel, TelegramChannel};
use nanobot_core::config::factory::{create_provider, resolve_workspace_path};
use nanobot_core::config::schema::Config;
use nanobot_core::cron::CronService;

/// Start the full gateway server: AgentLoop + ChannelManager + CronService.
///
/// All components run concurrently. Graceful shutdown on Ctrl+C.
pub async fn run(config: Config) -> Result<()> {
    info!("Starting nanobot gateway...");

    // 1. Create bus
    let (bus, inbound_rx, outbound_rx) = MessageBus::new();

    // 2. Create provider
    let provider = create_provider(&config)?;

    // 3. Create AgentLoop
    let mut agent = AgentLoop::from_config(config.clone(), bus.clone(), provider, inbound_rx);

    // 4. Create ChannelManager + register enabled channels
    let mut channel_mgr = ChannelManager::new(bus.clone(), outbound_rx);

    if config.channels.telegram.enabled {
        let telegram = TelegramChannel::new(config.channels.telegram.clone(), bus.clone());
        channel_mgr.add_channel(Box::new(telegram));
        info!("Telegram channel registered");
    }

    if config.channels.feishu.enabled {
        let feishu = FeishuChannel::new(config.channels.feishu.clone(), bus.clone());
        channel_mgr.add_channel(Box::new(feishu));
        info!("Feishu channel registered");
    }

    #[cfg(feature = "whatsapp")]
    if config.channels.whatsapp.enabled {
        let whatsapp = nanobot_core::channels::WhatsAppChannel::new(
            config.channels.whatsapp.clone(),
            bus.clone(),
        );
        channel_mgr.add_channel(Box::new(whatsapp));
        info!("WhatsApp channel registered");
    }

    // 5. Start channels
    channel_mgr.start_all().await?;

    // 6. Create CronService
    let workspace = resolve_workspace_path(&config.agents.defaults.workspace);
    let cron_path = workspace.join("cron_jobs.json");
    let mut cron_service = CronService::new(cron_path);
    let _ = cron_service.start();

    // 7. Show status
    let channel_names = channel_mgr.channel_names();
    println!("nanobot gateway started");
    println!("  Model:    {}", config.agents.defaults.model);
    println!(
        "  Channels: {}",
        if channel_names.is_empty() {
            "none".to_string()
        } else {
            channel_names.join(", ")
        }
    );
    println!("  Cron:     {} jobs", cron_service.status().job_count);
    println!("\nPress Ctrl+C to stop.\n");

    // 8. Spawn tasks
    let agent_handle = tokio::spawn(async move {
        agent.run().await;
    });

    let dispatch_handle = tokio::spawn(async move {
        channel_mgr.run_dispatch().await;
    });

    // 9. Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    info!("Shutdown signal received");
    println!("\nShutting down...");

    // 10. Graceful shutdown — drop bus to close channels
    cron_service.stop();
    drop(bus);

    // Wait for tasks to finish (they'll exit when bus channels close)
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), agent_handle).await;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), dispatch_handle).await;

    println!("nanobot gateway stopped.");
    Ok(())
}
