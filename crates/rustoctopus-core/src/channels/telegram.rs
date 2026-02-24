use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use tokio::sync::Mutex;
use tracing::{info, warn};

use super::traits::Channel;
use crate::bus::events::{InboundMessage, OutboundMessage};
use crate::bus::queue::MessageBus;
use crate::config::schema::TelegramConfig;

/// Maximum message length for Telegram (leave margin for formatting).
const MAX_MESSAGE_LEN: usize = 4000;

/// Telegram channel implementation using `teloxide`.
pub struct TelegramChannel {
    config: TelegramConfig,
    bus: MessageBus,
    running: Arc<AtomicBool>,
    bot: Option<Bot>,
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl TelegramChannel {
    pub fn new(config: TelegramConfig, bus: MessageBus) -> Self {
        Self {
            config,
            bus,
            running: Arc::new(AtomicBool::new(false)),
            bot: None,
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str {
        "telegram"
    }

    async fn start(&mut self) -> Result<()> {
        let bot = Bot::new(&self.config.token);
        self.bot = Some(bot.clone());
        self.running.store(true, Ordering::SeqCst);

        let bus = self.bus.clone();
        let allow_from = self.config.allow_from.clone();
        let running = Arc::clone(&self.running);
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        tokio::spawn(async move {
            info!("Telegram channel polling started");

            let handler = Update::filter_message().endpoint(
                move |bot: Bot, msg: Message, bus: Arc<MessageBus>, allow_from: Arc<Vec<String>>| {
                    let bus = (*bus).clone();
                    let allow_from = (*allow_from).clone();
                    async move {
                        handle_message(bot, msg, bus, &allow_from).await;
                        Ok::<(), teloxide::RequestError>(())
                    }
                },
            );

            let mut dispatcher = Dispatcher::builder(bot, handler)
                .dependencies(dptree::deps![
                    Arc::new(bus),
                    Arc::new(allow_from)
                ])
                .build();

            tokio::select! {
                _ = dispatcher.dispatch() => {
                    info!("Telegram dispatcher ended");
                }
                _ = &mut shutdown_rx => {
                    info!("Telegram channel received shutdown signal");
                    dispatcher.shutdown_token().shutdown().expect("shutdown failed").await;
                }
            }

            running.store(false, Ordering::SeqCst);
            info!("Telegram channel polling stopped");
        });

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(());
        }
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<()> {
        let Some(bot) = &self.bot else {
            anyhow::bail!("Telegram bot not initialized");
        };

        let chat_id: i64 = msg.chat_id.parse().map_err(|e| {
            anyhow::anyhow!("Invalid Telegram chat_id '{}': {}", msg.chat_id, e)
        })?;

        for chunk in split_message(&msg.content, MAX_MESSAGE_LEN) {
            // Try sending as HTML first, fall back to plain text
            let result = bot
                .send_message(ChatId(chat_id), &chunk)
                .parse_mode(ParseMode::Html)
                .await;

            if result.is_err() {
                // Fallback to plain text
                if let Err(e) = bot.send_message(ChatId(chat_id), &chunk).await {
                    warn!("Failed to send Telegram message: {}", e);
                }
            }
        }

        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Check if a sender is allowed based on the allow_from list.
///
/// Supports composite sender IDs with `|` separator (e.g. "12345|username").
fn is_allowed(sender_id: &str, allow_from: &[String]) -> bool {
    if allow_from.is_empty() {
        return true;
    }
    if allow_from.iter().any(|a| a == sender_id) {
        return true;
    }
    // Check composite parts
    if sender_id.contains('|') {
        for part in sender_id.split('|') {
            if !part.is_empty() && allow_from.iter().any(|a| a == part) {
                return true;
            }
        }
    }
    false
}

/// Build a composite sender ID from user info: "user_id|username".
fn build_sender_id(user: &teloxide::types::User) -> String {
    match &user.username {
        Some(username) => format!("{}|{}", user.id, username),
        None => user.id.to_string(),
    }
}

/// Handle an incoming Telegram message.
async fn handle_message(bot: Bot, msg: Message, bus: MessageBus, allow_from: &[String]) {
    let Some(user) = msg.from.as_ref() else { return };
    let sender_id = build_sender_id(user);
    let chat_id = msg.chat.id.to_string();

    // Handle /start command (no ACL check)
    if let Some(text) = msg.text() {
        if text == "/start" {
            let greeting = format!(
                "Hi {}! I'm RustOctopus.\n\nSend me a message and I'll respond!\nType /help to see available commands.",
                user.first_name
            );
            let _ = bot.send_message(msg.chat.id, greeting).await;
            return;
        }

        // Handle /help command (no ACL check)
        if text == "/help" {
            let help = "RustOctopus commands:\n/new — Start a new conversation\n/help — Show available commands";
            let _ = bot.send_message(msg.chat.id, help).await;
            return;
        }
    }

    // ACL check
    if !is_allowed(&sender_id, allow_from) {
        warn!(
            "Access denied for sender {} on Telegram. Add to allowFrom in config.",
            sender_id
        );
        return;
    }

    // Extract text content
    let content = msg
        .text()
        .or_else(|| msg.caption())
        .unwrap_or("[empty message]")
        .to_string();

    let inbound = InboundMessage::new("telegram", &sender_id, &chat_id, &content);
    bus.publish_inbound(inbound).await;
}

/// Split a message into chunks respecting Telegram's message length limit.
///
/// Prefers splitting on newlines, then spaces, then at max length.
fn split_message(content: &str, max_len: usize) -> Vec<String> {
    if content.len() <= max_len {
        return vec![content.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = content;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        let cut = &remaining[..max_len];

        // Prefer splitting at newline
        let pos = cut.rfind('\n').or_else(|| cut.rfind(' ')).unwrap_or(max_len);

        chunks.push(remaining[..pos].to_string());
        remaining = remaining[pos..].trim_start();
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_allowed_empty_list() {
        assert!(is_allowed("anyone", &[]));
    }

    #[test]
    fn test_is_allowed_direct_match() {
        let list = vec!["12345".to_string(), "user1".to_string()];
        assert!(is_allowed("12345", &list));
        assert!(is_allowed("user1", &list));
        assert!(!is_allowed("other", &list));
    }

    #[test]
    fn test_is_allowed_composite() {
        let list = vec!["12345".to_string()];
        assert!(is_allowed("12345|username", &list));
    }

    #[test]
    fn test_is_allowed_composite_by_username() {
        let list = vec!["myuser".to_string()];
        assert!(is_allowed("99999|myuser", &list));
    }

    #[test]
    fn test_is_allowed_composite_no_match() {
        let list = vec!["other".to_string()];
        assert!(!is_allowed("12345|username", &list));
    }

    #[test]
    fn test_split_message_short() {
        let chunks = split_message("hello", 100);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn test_split_message_at_newline() {
        let msg = format!("{}\n{}", "a".repeat(50), "b".repeat(50));
        let chunks = split_message(&msg, 60);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "a".repeat(50));
        assert_eq!(chunks[1], "b".repeat(50));
    }

    #[test]
    fn test_split_message_at_space() {
        let msg = format!("{} {}", "a".repeat(50), "b".repeat(50));
        let chunks = split_message(&msg, 60);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_split_message_no_break() {
        let msg = "a".repeat(200);
        let chunks = split_message(&msg, 100);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 100);
        assert_eq!(chunks[1].len(), 100);
    }
}
