use anyhow::Result;
use async_trait::async_trait;

use crate::bus::events::OutboundMessage;

/// A communication channel (Telegram, Feishu, etc.) that can receive
/// and send messages.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Returns the channel name (e.g. "telegram", "feishu").
    fn name(&self) -> &str;

    /// Start the channel (begin listening for messages).
    async fn start(&mut self) -> Result<()>;

    /// Stop the channel.
    async fn stop(&mut self) -> Result<()>;

    /// Send an outbound message through this channel.
    async fn send(&self, msg: OutboundMessage) -> Result<()>;

    /// Whether the channel is currently running.
    fn is_running(&self) -> bool;
}
