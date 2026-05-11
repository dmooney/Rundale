//! OllamaProcess server lifecycle management.

use parish_types::ParishError;
use std::process::{Child, Command};
use std::time::Duration;

/// Manages an Ollama server process started by Parish.
///
/// If Ollama was not already running when the game started, this struct
/// holds the child process handle. When dropped, it kills the process
/// to clean up. If Ollama was already running, this is a no-op wrapper.
pub struct OllamaProcess {
    child: Option<Child>,
}

impl OllamaProcess {
    /// Creates a no-op process handle (for non-Ollama providers).
    pub fn none() -> Self {
        Self { child: None }
    }

    /// Checks if Ollama is reachable. If not, starts `ollama serve` in the
    /// background and waits for it to become ready (up to 30 seconds).
    ///
    /// The optional `gpu_env` parameter allows injecting environment variables
    /// into the spawned process (e.g. `OLLAMA_VULKAN=1` for AMD GPUs on Windows).
    /// These are only applied when Parish starts Ollama itself; if Ollama is
    /// already running, the caller should restart it manually to change env vars.
    ///
    /// Returns an `OllamaProcess` that will stop the server on drop if
    /// we started it.
    pub async fn ensure_running(
        base_url: &str,
        gpu_env: Option<&[(String, String)]>,
    ) -> Result<Self, ParishError> {
        if Self::is_reachable(base_url).await {
            tracing::info!("Ollama already running at {}", base_url);
            return Ok(Self { child: None });
        }

        tracing::info!("Ollama not detected, starting ollama serve...");

        let mut cmd = Command::new("ollama");
        cmd.arg("serve")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        if let Some(env_vars) = gpu_env {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        let child = cmd.spawn().map_err(|e| {
            ParishError::Inference(format!(
                "failed to start ollama serve: {}. Is ollama installed?",
                e
            ))
        })?;

        // Wait for Ollama to become reachable
        let mut ready = false;
        for i in 0..60 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if Self::is_reachable(base_url).await {
                tracing::info!("Ollama ready after ~{}ms", (i + 1) * 500);
                ready = true;
                break;
            }
        }

        if !ready {
            return Err(ParishError::Inference(
                "ollama serve started but did not become reachable within 30s".to_string(),
            ));
        }

        Ok(Self { child: Some(child) })
    }

    /// Returns whether we started the Ollama process (vs. it was already running).
    pub fn was_started_by_us(&self) -> bool {
        self.child.is_some()
    }

    /// Checks if the Ollama API is reachable by hitting the root endpoint.
    async fn is_reachable(base_url: &str) -> bool {
        // Use the shared builder helper so a failing reqwest build falls
        // back to a default client instead of panicking (#98).
        let client = crate::openai_client::build_client_or_fallback(
            Duration::from_secs(2),
            "Ollama reachability probe",
        );
        client.get(base_url).send().await.is_ok()
    }

    /// Stops the Ollama process if we started it.
    ///
    /// On Windows, uses `taskkill /F /T /PID` to kill the entire process
    /// tree, ensuring GPU worker processes are also terminated and VRAM
    /// is released. On other platforms, uses the standard `kill()`.
    pub fn stop(&mut self) {
        if let Some(ref mut child) = self.child {
            tracing::info!("Stopping Ollama server...");

            #[cfg(target_os = "windows")]
            {
                let pid = child.id();
                // Kill the entire process tree so GPU workers release VRAM
                let _ = Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }

            #[cfg(not(target_os = "windows"))]
            {
                let _ = child.kill();
            }

            let _ = child.wait();
            self.child = None;
        }
    }
}

impl Drop for OllamaProcess {
    fn drop(&mut self) {
        self.stop();
    }
}
