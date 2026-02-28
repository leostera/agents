use teloxide::prelude::*;
use teloxide::types::ChatAction;
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};

use super::TELEGRAM_TYPING_REFRESH_SECS;

pub(super) struct TypingLoop {
    handle: JoinHandle<()>,
}

impl TypingLoop {
    pub(super) fn start(bot: Bot, chat_id: ChatId) -> Self {
        let handle = tokio::spawn(async move {
            loop {
                let _ = bot.send_chat_action(chat_id, ChatAction::Typing).await;
                sleep(Duration::from_secs(TELEGRAM_TYPING_REFRESH_SECS)).await;
            }
        });
        Self { handle }
    }
}

impl Drop for TypingLoop {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
