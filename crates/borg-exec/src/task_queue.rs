use std::sync::Arc;

use anyhow::{Result, anyhow};
use borg_core::Uri;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[derive(Clone)]
pub struct TaskQueue {
    sender: UnboundedSender<Uri>,
    receiver: Arc<Mutex<UnboundedReceiver<Uri>>>,
}

impl TaskQueue {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded_channel::<Uri>();
        Self {
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    pub async fn queue(&self, task_id: Uri) -> Result<()> {
        self.sender
            .send(task_id)
            .map_err(|_| anyhow!("task queue is closed"))
    }

    pub async fn next(&self) -> Result<Uri> {
        let mut receiver = self.receiver.lock().await;
        receiver
            .recv()
            .await
            .ok_or_else(|| anyhow!("task queue receiver is closed"))
    }
}
