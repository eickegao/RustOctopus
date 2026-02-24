pub mod events;
pub mod queue;

pub use events::{InboundMessage, OutboundMessage};
pub use queue::MessageBus;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bus_inbound_send_recv() {
        let (bus, mut inbound_rx, _outbound_rx) = MessageBus::new();

        let msg = InboundMessage::new("telegram", "user1", "chat42", "hello world");
        bus.publish_inbound(msg).await;

        let received = inbound_rx.recv().await.expect("should receive inbound message");
        assert_eq!(received.content, "hello world");
        assert_eq!(received.channel, "telegram");
        assert_eq!(received.sender_id, "user1");
        assert_eq!(received.chat_id, "chat42");
    }

    #[tokio::test]
    async fn test_bus_outbound_send_recv() {
        let (_bus, _inbound_rx, mut outbound_rx) = MessageBus::new();

        let msg = OutboundMessage::new("telegram", "chat42", "reply text");
        _bus.publish_outbound(msg).await;

        let received = outbound_rx.recv().await.expect("should receive outbound message");
        assert_eq!(received.content, "reply text");
        assert_eq!(received.channel, "telegram");
        assert_eq!(received.chat_id, "chat42");
    }

    #[test]
    fn test_session_key() {
        let msg = InboundMessage::new("telegram", "user1", "chat42", "hi");
        assert_eq!(msg.session_key(), "telegram:chat42");
    }

    #[test]
    fn test_session_key_override() {
        let mut msg = InboundMessage::new("telegram", "user1", "chat42", "hi");
        msg.session_key_override = Some("custom-session-key".to_string());
        assert_eq!(msg.session_key(), "custom-session-key");
    }

    #[test]
    fn test_inbound_message_new_defaults() {
        let msg = InboundMessage::new("cli", "user1", "chat1", "test");
        assert!(msg.media.is_empty());
        assert!(msg.metadata.is_empty());
        assert!(msg.session_key_override.is_none());
    }
}
