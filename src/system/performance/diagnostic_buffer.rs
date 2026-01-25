//! Non-blocking Diagnostic Buffer
//!
//! Provides a lock-free mechanism for collecting diagnostic messages
//! without blocking high-precision measurement loops.
//!
//! Uses crossbeam_channel for thread-safe, non-blocking message passing
//! between measurement threads and a background consumer thread.
//!
//! Also provides event consumer that reads CollectorEvent's from an rtrb ring buffer
//! and formats them into diagnostic messages for asynchronous logging.

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// A diagnostic message to be logged
#[derive(Clone, Debug)]
pub struct DiagnosticMessage {
    /// The log message content
    pub message: String,
    /// Timestamp when the message was created
    pub timestamp: std::time::Instant,
}

/// Non-blocking diagnostic buffer using crossbeam channels
pub struct DiagnosticBuffer {
    /// Sender side of the channel for non-blocking sends
    sender: Option<Sender<DiagnosticMessage>>,
    /// Receiver side (held internally for background consumer)
    receiver: Option<Receiver<DiagnosticMessage>>,
    /// Handle to the background consumer thread
    consumer_thread: Option<thread::JoinHandle<()>>,
    /// Stop flag for the consumer thread
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    /// Channel capacity (max pending messages)
    capacity: usize,
}

impl DiagnosticBuffer {
    /// Creates a new diagnostic buffer with the specified capacity
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of pending messages before blocking
    ///   (typically 1024-4096 for measurement loops)
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = bounded(capacity);
        DiagnosticBuffer {
            sender: Some(sender),
            receiver: Some(receiver),
            consumer_thread: None,
            stop_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            capacity,
        }
    }

    /// Starts the background consumer thread
    ///
    /// The consumer thread continuously reads messages from the channel
    /// and writes them to stderr. This should be called once before
    /// using `send()` in measurement loops.
    pub fn start_consumer(&mut self) {
        if let Some(receiver) = self.receiver.take() {
            let stop_flag = self.stop_flag.clone();
            let consumer_thread = thread::spawn(move || {
                while !stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    // Try to receive with a timeout to avoid blocking forever
                    match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(msg) => {
                            // Write directly to stderr for minimal latency
                            eprintln!("{}", msg.message);
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                            // Timeout is fine, continue looping
                            continue;
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                            // Sender was dropped, consumer can exit
                            break;
                        }
                    }
                }
            });
            self.consumer_thread = Some(consumer_thread);
        }
    }

    /// Non-blocking send of a diagnostic message
    ///
    /// Returns `Ok(())` if the message was queued successfully.
    /// Returns `Err` if the channel is full (measurement thread should continue without blocking).
    ///
    /// # Arguments
    /// * `message` - The log message to queue
    pub fn send(&self, message: &str) -> Result<(), TrySendError<DiagnosticMessage>> {
        if let Some(ref sender) = self.sender {
            let msg = DiagnosticMessage {
                message: message.to_string(),
                timestamp: std::time::Instant::now(),
            };
            sender.try_send(msg)
        } else {
            Err(TrySendError::Disconnected(DiagnosticMessage {
                message: message.to_string(),
                timestamp: std::time::Instant::now(),
            }))
        }
    }

    /// Flushes pending messages (waits for consumer to catch up)
    pub fn flush(&self) {
        // Consumer thread processes continuously, so flush is implicit
        // We can add a small sleep to ensure pending messages are processed
        thread::sleep(Duration::from_millis(10));
    }

    /// Returns the current capacity of the buffer
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Gracefully shuts down the diagnostic buffer
    pub fn shutdown(self) {
        // Drop self to trigger Drop implementation which handles cleanup
        drop(self);
    }
}

impl Drop for DiagnosticBuffer {
    fn drop(&mut self) {
        // Signal the consumer thread to stop
        self.stop_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);

        // Explicitly drop the sender to signal the consumer to exit
        self.sender.take();

        if let Some(thread) = self.consumer_thread.take() {
            // Wait for the consumer thread to finish
            let _ = thread.join();
        }
    }
}

lazy_static::lazy_static! {
    /// Global diagnostic buffer instance (singleton)
    static ref GLOBAL_DIAGNOSTIC_BUFFER: std::sync::Mutex<Option<Arc<DiagnosticBuffer>>> = {
        std::sync::Mutex::new(None)
    };
}

/// Initialize the global diagnostic buffer
pub fn init_global_buffer(capacity: usize) -> Arc<DiagnosticBuffer> {
    let mut buffer_ref = GLOBAL_DIAGNOSTIC_BUFFER.lock().unwrap();
    if buffer_ref.is_none() {
        let mut buffer = DiagnosticBuffer::new(capacity);
        buffer.start_consumer();
        let arc = Arc::new(buffer);
        *buffer_ref = Some(arc.clone());
        arc
    } else {
        buffer_ref.as_ref().unwrap().clone()
    }
}

/// Get the global diagnostic buffer
/// Returns `None` if not initialized (caller should handle gracefully)
pub fn get_global_buffer() -> Option<Arc<DiagnosticBuffer>> {
    let buffer_ref = GLOBAL_DIAGNOSTIC_BUFFER.lock().unwrap();
    buffer_ref.as_ref().cloned()
}

/// Safe helper: Send a diagnostic message, silently ignoring if buffer not initialized
///
/// This allows code to safely call diagnostic logging even before the buffer
/// is initialized, preventing panics in early initialization or edge cases.
pub fn send_diagnostic(message: &str) {
    if let Some(buffer) = get_global_buffer() {
        let _ = buffer.send(message);
    }
    // If buffer not initialized, silently continue (no panic, no error)
}

/// Spawns a background consumer thread that reads CollectorEvent's from an rtrb ring buffer
/// and formats them into diagnostic messages for asynchronous logging.
///
/// This consumer runs outside the critical path and handles all formatting,
/// allowing the hot loop to remain strictly lock-free and allocation-free.
///
/// Implements asynchronous SMI correlation by maintaining state across spike events
/// and comparing raw SMI counts to detect actual SMI-correlated spikes.
///
/// # Arguments
/// * `event_consumer` - The rtrb Consumer<CollectorEvent> to read from
/// * `smi_correlated_spikes` - Atomic counter to increment when SMI correlation is detected
///
/// # Returns
/// A JoinHandle for the consumer thread (can be dropped to allow background operation)
pub fn spawn_collector_event_consumer(
    event_consumer: rtrb::Consumer<super::collector::CollectorEvent>,
    smi_correlated_spikes: std::sync::Arc<std::sync::atomic::AtomicU64>,
) -> std::thread::JoinHandle<()> {
    // Determine if we are in a test environment to avoid infinite loop leaks
    let is_test = cfg!(test);

    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut local_consumer = event_consumer;
            // Initialize last_smi_count using the current value of TOTAL_SMI_COUNT
            // This avoids false-positives on the first detected spike if the system SMI count is non-zero
            let mut last_smi_count =
                super::collector::TOTAL_SMI_COUNT.load(std::sync::atomic::Ordering::Relaxed);

            let mut empty_count = 0;
            loop {
                // Try to read an event from the ring buffer
                match local_consumer.pop() {
                    Ok(event) => {
                        empty_count = 0;
                        // Format and send the event as a diagnostic message
                        let msg = match event {
                            super::collector::CollectorEvent::Spike(latency_ns, spike_number, raw_smi_count) => {
                                // Asynchronous SMI correlation: compare raw_smi_count with last_smi_count
                                let is_smi_correlated = raw_smi_count > last_smi_count;

                                let msg = if is_smi_correlated {
                                    smi_correlated_spikes.fetch_add(1, std::sync::atomic::Ordering::Release);
                                    let smi_delta = raw_smi_count - last_smi_count;
                                    format!(
                                        "[COLLECTOR_EVENT] Spike #{}: latency={} ns (SMI-correlated, smi_count_delta={})",
                                        spike_number, latency_ns, smi_delta
                                    )
                                } else {
                                    format!(
                                        "[COLLECTOR_EVENT] Spike #{}: latency={} ns (threshold boundary)",
                                        spike_number, latency_ns
                                    )
                                };

                                // Update last_smi_count for next spike comparison
                                last_smi_count = raw_smi_count;
                                msg
                            }
                            super::collector::CollectorEvent::SmiDetected => {
                                "[COLLECTOR_EVENT] SMI-correlated spike detected".to_string()
                            }
                            super::collector::CollectorEvent::BufferFull(dropped_count) => {
                                format!(
                                    "[COLLECTOR_EVENT] ⚠ Ring buffer full! Total dropped: {}",
                                    dropped_count
                                )
                            }
                            super::collector::CollectorEvent::Status {
                                samples,
                                spikes,
                                smi_correlated,
                                dropped,
                            } => {
                                format!(
                                    "[COLLECTOR_EVENT] Status update: Samples={}, Spikes={}, SMI-correlated={}, Dropped={}",
                                    samples, spikes, smi_correlated, dropped
                                )
                            }
                            super::collector::CollectorEvent::WarmupComplete => {
                                "[COLLECTOR_EVENT] Warmup complete - transitioning to official metrics recording".to_string()
                            }
                            super::collector::CollectorEvent::Flush => {
                                "[COLLECTOR_EVENT] Warmup complete - transitioning to official metrics recording".to_string()
                            }
                        };
                        send_diagnostic(&msg);
                    }
                    Err(_) => {
                        // Ring buffer is empty, sleep briefly before trying again
                        std::thread::sleep(Duration::from_micros(100));

                        // In tests, exit if we've been empty for a while to avoid leaks
                        if is_test {
                            empty_count += 1;
                            if empty_count > 100 {
                                // ~10ms of emptiness in test
                                break;
                            }
                        }
                    }
                }
            }
        }));

        // Handle panic result: log and continue gracefully
        if let Err(panic_info) = result {
            let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                format!("[COLLECTOR_EVENT_CONSUMER] PANIC: {}", s)
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                format!("[COLLECTOR_EVENT_CONSUMER] PANIC: {}", s)
            } else {
                "[COLLECTOR_EVENT_CONSUMER] PANIC: unknown panic info".to_string()
            };
            send_diagnostic(&panic_msg);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_buffer_creation() {
        let buffer = DiagnosticBuffer::new(256);
        assert_eq!(buffer.capacity(), 256);
    }

    #[test]
    fn test_diagnostic_buffer_send() {
        let mut buffer = DiagnosticBuffer::new(256);
        buffer.start_consumer();

        // Should not block
        let result = buffer.send("[TEST] Non-blocking send");
        assert!(result.is_ok());

        buffer.flush();
    }

    #[test]
    fn test_diagnostic_buffer_multiple_sends() {
        let mut buffer = DiagnosticBuffer::new(256);
        buffer.start_consumer();

        for i in 0..10 {
            let msg = format!("[TEST] Message {}", i);
            assert!(buffer.send(&msg).is_ok());
        }

        buffer.flush();
    }
}
