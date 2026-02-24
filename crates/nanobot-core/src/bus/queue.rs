use tokio::sync::mpsc;

use super::events::{InboundMessage, OutboundMessage};

#[derive(Clone)]
pub struct MessageBus {
    inbound_tx: mpsc::UnboundedSender<InboundMessage>,
    outbound_tx: mpsc::UnboundedSender<OutboundMessage>,
}

impl MessageBus {
    /// Creates a new `MessageBus` and returns the bus together with the
    /// receiving halves for inbound and outbound messages.
    pub fn new() -> (
        Self,
        mpsc::UnboundedReceiver<InboundMessage>,
        mpsc::UnboundedReceiver<OutboundMessage>,
    ) {
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();

        let bus = Self {
            inbound_tx,
            outbound_tx,
        };

        (bus, inbound_rx, outbound_rx)
    }

    /// Publishes an inbound message onto the bus.
    pub async fn publish_inbound(&self, msg: InboundMessage) {
        let _ = self.inbound_tx.send(msg);
    }

    /// Publishes an outbound message onto the bus.
    pub async fn publish_outbound(&self, msg: OutboundMessage) {
        let _ = self.outbound_tx.send(msg);
    }
}
