//! Setup progress reporting — the [`SetupProgress`] trait and its
//! built-in [`StdoutProgress`] implementation.

/// Trait for reporting setup progress to the UI layer.
///
/// Implemented differently by headless and other modes to show
/// installation, detection, and download progress appropriately.
pub trait SetupProgress: Send + Sync {
    /// Reports a status message during setup.
    fn on_status(&self, msg: &str);
    /// Reports aggregate model pull progress (bytes downloaded vs total).
    fn on_pull_progress(&self, completed: u64, total: u64);
    /// Reports an error during setup.
    fn on_error(&self, msg: &str);
}

/// A simple progress reporter that prints to stdout.
pub struct StdoutProgress;

impl SetupProgress for StdoutProgress {
    fn on_status(&self, msg: &str) {
        println!("[Parish] {}", msg);
    }

    fn on_pull_progress(&self, completed: u64, total: u64) {
        if total > 0 {
            let pct = (completed as f64 / total as f64) * 100.0;
            print!("\r[Parish] The tale is {:.1}% arrived...", pct);
            if completed >= total {
                println!();
            }
        }
    }

    fn on_error(&self, msg: &str) {
        eprintln!("[Parish] ERROR: {}", msg);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn test_stdout_progress_on_status() {
        // Just verify it doesn't panic
        let progress = StdoutProgress;
        progress.on_status("test message");
    }

    #[test]
    fn test_stdout_progress_on_error() {
        let progress = StdoutProgress;
        progress.on_error("test error");
    }

    /// Tracks status messages for testing.
    pub struct TestProgress {
        pub messages: std::sync::Mutex<Vec<String>>,
    }

    impl TestProgress {
        pub fn new() -> Self {
            Self {
                messages: std::sync::Mutex::new(Vec::new()),
            }
        }

        pub fn messages(&self) -> Vec<String> {
            self.messages.lock().unwrap().clone()
        }
    }

    impl SetupProgress for TestProgress {
        fn on_status(&self, msg: &str) {
            self.messages.lock().unwrap().push(msg.to_string());
        }

        fn on_pull_progress(&self, completed: u64, total: u64) {
            self.messages
                .lock()
                .unwrap()
                .push(format!("progress: {}/{}", completed, total));
        }

        fn on_error(&self, msg: &str) {
            self.messages
                .lock()
                .unwrap()
                .push(format!("ERROR: {}", msg));
        }
    }

    #[test]
    fn test_test_progress_tracks_messages() {
        let progress = TestProgress::new();
        progress.on_status("hello");
        progress.on_status("world");
        progress.on_pull_progress(50, 100);
        progress.on_error("oops");

        let msgs = progress.messages();
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0], "hello");
        assert_eq!(msgs[1], "world");
        assert_eq!(msgs[2], "progress: 50/100");
        assert_eq!(msgs[3], "ERROR: oops");
    }
}
