use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;
use tracing::info;

use rustoctopus_core::agent::agent_loop::AgentLoop;
use rustoctopus_core::bus::queue::MessageBus;
use rustoctopus_core::channels::{ChannelManager, FeishuChannel, TelegramChannel};
use rustoctopus_core::config::factory::{create_provider, resolve_workspace_path};
use rustoctopus_core::config::loader::load_config;
use rustoctopus_core::config::schema::Config;
use rustoctopus_core::cron::CronService;
use rustoctopus_core::mcp::manager::McpManager;

pub struct AppState {
    pub config: Mutex<Config>,
    pub cron: Mutex<CronService>,
    pub channel_names: Mutex<Vec<String>>,
    pub started_at: Instant,
    pub mcp_manager: Mutex<McpManager>,
}

impl AppState {
    pub async fn boot() -> anyhow::Result<Arc<Self>> {
        let config = load_config(None)?;

        let (bus, inbound_rx, outbound_rx) = MessageBus::new();
        let provider = create_provider(&config)?;
        let mut agent = AgentLoop::from_config(config.clone(), bus.clone(), provider, inbound_rx);

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
            let whatsapp = rustoctopus_core::channels::WhatsAppChannel::new(
                config.channels.whatsapp.clone(),
                bus.clone(),
            );
            channel_mgr.add_channel(Box::new(whatsapp));
            info!("WhatsApp channel registered");
        }

        let names = channel_mgr.channel_names();
        channel_mgr.start_all().await?;

        let workspace = resolve_workspace_path(&config.agents.defaults.workspace);
        let cron_path = workspace.join("cron_jobs.json");
        let mut cron_service = CronService::new(cron_path);
        let _ = cron_service.start();

        let mut mcp_manager = McpManager::new();
        if config.mcp.enabled {
            mcp_manager.start_all(&config.mcp.servers).await;
        }

        info!(
            model = %config.agents.defaults.model,
            channels = ?names,
            cron_jobs = cron_service.status().job_count,
            mcp_servers = mcp_manager.running_count(),
            "Gateway started"
        );

        tokio::spawn(async move { agent.run().await });
        tokio::spawn(async move { channel_mgr.run_dispatch().await });

        Ok(Arc::new(Self {
            config: Mutex::new(config),
            cron: Mutex::new(cron_service),
            channel_names: Mutex::new(names),
            started_at: Instant::now(),
            mcp_manager: Mutex::new(mcp_manager),
        }))
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}
