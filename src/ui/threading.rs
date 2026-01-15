/// Threading and Async Integration Helpers
///
/// Utilities for managing tokio task spawning, channel communication,
/// and async event handling within the egui event loop.

use tokio::sync::mpsc;

/// Wrapper for managing async tasks and their event channels
pub struct AsyncBridge {
    /// Channel sender for build events
    pub build_tx: mpsc::Sender<crate::ui::controller::BuildEvent>,
    
    /// Cancellation signal (watch channel)
    pub cancel_tx: tokio::sync::watch::Sender<bool>,
    
    /// Cancellation receiver
    pub cancel_rx: tokio::sync::watch::Receiver<bool>,
}

impl AsyncBridge {
    /// Create a new async bridge with configured channel sizes
    pub fn new() -> (Self, mpsc::Receiver<crate::ui::controller::BuildEvent>) {
        let (build_tx, build_rx) = mpsc::channel(100);
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        
        let bridge = Self {
            build_tx,
            cancel_tx,
            cancel_rx,
        };
        
        (bridge, build_rx)
    }
    
    /// Signal cancellation to background tasks
    pub async fn signal_cancel(&self) -> Result<(), tokio::sync::watch::error::SendError<bool>> {
        self.cancel_tx.send(true)
    }
    
    /// Reset cancellation signal
    pub async fn reset_cancel(&self) -> Result<(), tokio::sync::watch::error::SendError<bool>> {
        self.cancel_tx.send(false)
    }
}

impl Default for AsyncBridge {
    fn default() -> Self {
        Self::new().0
    }
}

/// Helper to spawn a non-blocking task that reports progress
pub fn spawn_monitored_task<F, Fut>(
    name: &'static str,
    tx: mpsc::Sender<crate::ui::controller::BuildEvent>,
    f: F,
) -> tokio::task::JoinHandle<()>
where
    F: FnOnce(mpsc::Sender<crate::ui::controller::BuildEvent>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send,
{
    tokio::spawn(async move {
        match f(tx).await {
            Ok(()) => {
                // Task completed successfully
            }
            Err(e) => {
                eprintln!("Task {} failed: {}", name, e);
            }
        }
    })
}

/// Request UI repaint from a background thread
pub fn request_ui_repaint(ctx: Option<&eframe::egui::Context>) {
    if let Some(c) = ctx {
        c.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_async_bridge_creation() {
        let (bridge, _rx) = AsyncBridge::new();
        assert!(bridge.signal_cancel().await.is_ok());
        
        // Verify cancel signal propagated
        let mut cancel_rx = bridge.cancel_rx.clone();
        assert!(cancel_rx.changed().await.is_ok());
    }
    
    #[tokio::test]
    async fn test_channel_send_receive() {
        let (bridge, mut rx) = AsyncBridge::new();
        
        let tx = bridge.build_tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(crate::ui::controller::BuildEvent::Status(
                "Test message".to_string(),
            )).await;
        });
        
        // Receive event
        if let Some(event) = rx.recv().await {
            match event {
                crate::ui::controller::BuildEvent::Status(msg) => {
                    assert_eq!(msg, "Test message");
                }
                _ => panic!("Unexpected event type"),
            }
        }
    }
}
