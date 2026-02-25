use std::sync::Arc;

use anyhow::{Result, anyhow};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[derive(Clone)]
pub struct TaskQueue {
    sender: UnboundedSender<String>,
    receiver: Arc<Mutex<UnboundedReceiver<String>>>,
}

impl TaskQueue {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded_channel::<String>();
        Self {
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    pub async fn queue(&self, task_id: String) -> Result<()> {
        self.sender
            .send(task_id)
            .map_err(|_| anyhow!("task queue is closed"))
    }

    pub async fn next(&self) -> Result<String> {
        let mut receiver = self.receiver.lock().await;
        receiver
            .recv()
            .await
            .ok_or_else(|| anyhow!("task queue receiver is closed"))
    }
}
