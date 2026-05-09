use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// Named stage starting (e.g., "Downloading libraries")
    Stage { name: String, total: Option<u64> },
    /// Incremental progress within current stage
    Advance { delta: u64 },
    /// Absolute progress within current stage
    Progress { current: u64, total: u64 },
    /// Informational log line
    Log { level: LogLevel, message: String },
    /// Current stage completed
    Done,
    /// Fatal error — operation aborted
    Error { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Cloneable handle for sending progress events from async tasks.
#[derive(Debug, Clone)]
pub struct ProgressReporter {
    tx: mpsc::Sender<ProgressEvent>,
}

impl ProgressReporter {
    pub fn new(tx: mpsc::Sender<ProgressEvent>) -> Self {
        Self { tx }
    }

    /// Create a no-op reporter that discards all events.
    pub fn noop() -> Self {
        let (tx, _rx) = mpsc::channel(1);
        Self { tx }
    }

    pub fn channel(buffer: usize) -> (Self, mpsc::Receiver<ProgressEvent>) {
        let (tx, rx) = mpsc::channel(buffer);
        (Self { tx }, rx)
    }

    pub async fn stage(&self, name: impl Into<String>, total: Option<u64>) {
        let _ = self.tx.send(ProgressEvent::Stage { name: name.into(), total }).await;
    }

    pub async fn advance(&self, delta: u64) {
        let _ = self.tx.send(ProgressEvent::Advance { delta }).await;
    }

    pub async fn progress(&self, current: u64, total: u64) {
        let _ = self.tx.send(ProgressEvent::Progress { current, total }).await;
    }

    pub async fn log(&self, level: LogLevel, message: impl Into<String>) {
        let _ = self.tx.send(ProgressEvent::Log { level, message: message.into() }).await;
    }

    pub async fn info(&self, message: impl Into<String>) {
        self.log(LogLevel::Info, message).await;
    }

    pub async fn warn(&self, message: impl Into<String>) {
        self.log(LogLevel::Warn, message).await;
    }

    pub async fn done(&self) {
        let _ = self.tx.send(ProgressEvent::Done).await;
    }

    pub async fn error(&self, message: impl Into<String>) {
        let _ = self.tx.send(ProgressEvent::Error { message: message.into() }).await;
    }
}
