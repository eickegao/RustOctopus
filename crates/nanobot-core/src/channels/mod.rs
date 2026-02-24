pub mod manager;
pub mod traits;

#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "feishu")]
pub mod feishu;

#[cfg(feature = "whatsapp")]
pub mod whatsapp;

pub use manager::ChannelManager;
pub use traits::Channel;

#[cfg(feature = "telegram")]
pub use telegram::TelegramChannel;

#[cfg(feature = "feishu")]
pub use feishu::FeishuChannel;

#[cfg(feature = "whatsapp")]
pub use whatsapp::WhatsAppChannel;
