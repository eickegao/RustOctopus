use std::collections::HashMap;

use anyhow::Result;
use tokio::sync::mpsc;
use tracing::{info, warn};

use super::traits::Channel;
use crate::bus::events::OutboundMessage;
use crate::bus::queue::MessageBus;

/// Manages all registered channels.
///
/// Consumes `outbound_rx` and dispatches messages to the appropriate channel
/// (or broadcasts to all if the outbound message's `channel` is empty).
pub struct ChannelManager {
    #[allow(dead_code)]
    bus: MessageBus,
    channels: HashMap<String, Box<dyn Channel>>,
    outbound_rx: mpsc::UnboundedReceiver<OutboundMessage>,
}

impl ChannelManager {
    pub fn new(bus: MessageBus, outbound_rx: mpsc::UnboundedReceiver<OutboundMessage>) -> Self {
        Self {
            bus,
            channels: HashMap::new(),
            outbound_rx,
        }
    }

    /// Register a channel.
    pub fn add_channel(&mut self, channel: Box<dyn Channel>) {
        let name = channel.name().to_string();
        self.channels.insert(name, channel);
    }

    /// Returns the names of all registered channels.
    pub fn channel_names(&self) -> Vec<String> {
        self.channels.keys().cloned().collect()
    }

    /// Start all registered channels.
    pub async fn start_all(&mut self) -> Result<()> {
        for (name, ch) in self.channels.iter_mut() {
            info!(channel = %name, "Starting channel");
            ch.start().await?;
        }
        Ok(())
    }

    /// Stop all registered channels.
    pub async fn stop_all(&mut self) {
        for (name, ch) in self.channels.iter_mut() {
            info!(channel = %name, "Stopping channel");
            if let Err(e) = ch.stop().await {
                warn!(channel = %name, error = %e, "Error stopping channel");
            }
        }
    }

    /// Run the dispatch loop: consume outbound messages and route to channels.
    ///
    /// Runs until the outbound sender is dropped (channel closed).
    pub async fn run_dispatch(&mut self) {
        info!("ChannelManager dispatch loop started");
        while let Some(msg) = self.outbound_rx.recv().await {
            self.dispatch(msg).await;
        }
        info!("ChannelManager dispatch loop ended (outbound channel closed)");
    }

    async fn dispatch(&self, msg: OutboundMessage) {
        let target_channel = msg.channel.clone();

        if target_channel.is_empty() {
            // Broadcast to all channels
            for (name, ch) in &self.channels {
                if let Err(e) = ch.send(msg.clone()).await {
                    warn!(channel = %name, error = %e, "Failed to send to channel");
                }
            }
        } else if let Some(ch) = self.channels.get(&target_channel) {
            if let Err(e) = ch.send(msg).await {
                warn!(channel = %target_channel, error = %e, "Failed to send to channel");
            }
        } else {
            warn!(channel = %target_channel, "No channel registered with this name");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::queue::MessageBus;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    struct MockChannel {
        name: String,
        started: Arc<AtomicBool>,
        sent: Arc<Mutex<Vec<OutboundMessage>>>,
    }

    impl MockChannel {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                started: Arc::new(AtomicBool::new(false)),
                sent: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn sent_messages(&self) -> Arc<Mutex<Vec<OutboundMessage>>> {
            Arc::clone(&self.sent)
        }
    }

    #[async_trait::async_trait]
    impl Channel for MockChannel {
        fn name(&self) -> &str {
            &self.name
        }
        async fn start(&mut self) -> Result<()> {
            self.started.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn stop(&mut self) -> Result<()> {
            self.started.store(false, Ordering::SeqCst);
            Ok(())
        }
        async fn send(&self, msg: OutboundMessage) -> Result<()> {
            self.sent.lock().unwrap().push(msg);
            Ok(())
        }
        fn is_running(&self) -> bool {
            self.started.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn test_manager_registers_channel() {
        let (bus, _inbound_rx, outbound_rx) = MessageBus::new();
        let mut mgr = ChannelManager::new(bus, outbound_rx);
        let ch = MockChannel::new("test");
        mgr.add_channel(Box::new(ch));
        assert_eq!(mgr.channel_names().len(), 1);
        assert!(mgr.channel_names().contains(&"test".to_string()));
    }

    #[tokio::test]
    async fn test_manager_start_stop_all() {
        let (bus, _inbound_rx, outbound_rx) = MessageBus::new();
        let mut mgr = ChannelManager::new(bus, outbound_rx);
        let ch = MockChannel::new("test");
        let started = Arc::clone(&ch.started);
        mgr.add_channel(Box::new(ch));

        mgr.start_all().await.unwrap();
        assert!(started.load(Ordering::SeqCst));

        mgr.stop_all().await;
        assert!(!started.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_manager_dispatches_to_specific_channel() {
        let (bus, _inbound_rx, outbound_rx) = MessageBus::new();

        let ch1 = MockChannel::new("telegram");
        let sent1 = ch1.sent_messages();
        let ch2 = MockChannel::new("feishu");
        let sent2 = ch2.sent_messages();

        // Publish a message before creating manager (sender stays alive via bus)
        bus.publish_outbound(OutboundMessage::new("telegram", "chat1", "hello telegram"))
            .await;

        // Create manager — this moves outbound_rx into it.
        // The manager also stores a bus clone (which has an outbound_tx).
        // We must drop both the external bus AND the manager's bus to close the channel.
        let mut mgr = ChannelManager::new(bus.clone(), outbound_rx);
        mgr.add_channel(Box::new(ch1));
        mgr.add_channel(Box::new(ch2));

        // Drop the external bus clone
        drop(bus);

        // Spawn dispatch, then wait briefly for it to process
        let handle = tokio::spawn(async move {
            mgr.run_dispatch().await;
            mgr // return manager so we can check results
        });

        // Give dispatch time to process, then it'll stop when bus drops
        // The manager's own bus clone keeps the channel alive, so we use timeout
        let result = tokio::time::timeout(std::time::Duration::from_millis(100), handle).await;

        // Dispatch is still running (manager holds bus clone), but message was delivered
        assert!(result.is_err()); // timeout = still running = expected
        assert_eq!(sent1.lock().unwrap().len(), 1);
        assert_eq!(sent1.lock().unwrap()[0].content, "hello telegram");
        assert_eq!(sent2.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_manager_broadcasts_empty_channel() {
        let (bus, _inbound_rx, outbound_rx) = MessageBus::new();

        let ch1 = MockChannel::new("telegram");
        let sent1 = ch1.sent_messages();
        let ch2 = MockChannel::new("feishu");
        let sent2 = ch2.sent_messages();

        // Publish with empty channel = broadcast
        bus.publish_outbound(OutboundMessage::new("", "chat1", "broadcast"))
            .await;

        let mut mgr = ChannelManager::new(bus.clone(), outbound_rx);
        mgr.add_channel(Box::new(ch1));
        mgr.add_channel(Box::new(ch2));
        drop(bus);

        let handle = tokio::spawn(async move {
            mgr.run_dispatch().await;
        });

        let _ = tokio::time::timeout(std::time::Duration::from_millis(100), handle).await;

        assert_eq!(sent1.lock().unwrap().len(), 1);
        assert_eq!(sent2.lock().unwrap().len(), 1);
    }
}
