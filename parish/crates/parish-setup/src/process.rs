//! Managed child-process handles for Ollama, vllm-mlx, and vllm.
//!
//! Each struct wraps an optional `Child` and ensures the server is stopped
//! when dropped. The `ensure_running` / `ensure_slots` constructors probe the
//! server endpoint first and only spawn a new process when necessary.

use parish_providers::openai_client::build_client_or_fallback;
use parish_types::ParishError;
use std::process::{Child, Command};
use std::time::Duration;

// ── Ollama ─────────────────────────────────────────────────────────────────

/// Manages an Ollama server process started by Parish.
///
/// If Ollama was not already running when the game started, this struct
/// holds the child process handle. When dropped, it kills the process
/// to clean up. If Ollama was already running, this is a no-op wrapper.
pub struct OllamaProcess {
    pub(super) child: Option<Child>,
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
            kill_and_reap(child);
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
        let client = build_client_or_fallback(Duration::from_secs(2), "Ollama reachability probe");
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
                let pid_arg = pid_string(child.id());
                let args = taskkill_args(&pid_arg);
                let _ = Command::new("taskkill")
                    .args(args)
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

/// Builds the `taskkill` argument list used by [`OllamaProcess::stop`] on
/// Windows. Force-kill (`/F`) the entire process tree (`/T`) for the given
/// `/PID`, releasing GPU worker VRAM that orphans otherwise hold. Returned as
/// `&str` slices so the call site can pass them straight to `Command::args`.
///
/// Pure (no `Command` invocation), cross-platform, and total — so the
/// invariant "we always pass `/F /T /PID <pid>`" can be regression-tested
/// without spawning processes or running on Windows. (TD-015)
#[cfg(any(target_os = "windows", test))]
fn taskkill_args(pid_arg: &str) -> [&str; 4] {
    ["/F", "/T", "/PID", pid_arg]
}

/// Formats a Windows process ID for the `taskkill /PID` argument.
/// Pure helper paired with [`taskkill_args`] for test isolation. (TD-015)
#[cfg(any(target_os = "windows", test))]
fn pid_string(pid: u32) -> String {
    pid.to_string()
}

/// Kills and reaps a spawned `Child` that never became reachable.
///
/// Called on each timeout-failure path where the `Child` is still a local
/// variable (not yet stored in `Self`), so `Drop` on `Self` cannot clean it
/// up. Without this call the child becomes an orphan/zombie.
///
/// On Windows, uses `taskkill /F /T /PID` to kill the entire process tree so
/// GPU worker sub-processes and multiprocessing workers (vllm) are also
/// terminated. On other platforms, uses the standard `kill()` + `wait()`.
fn kill_and_reap(mut child: Child) {
    #[cfg(target_os = "windows")]
    {
        let pid_arg = child.id().to_string();
        let args = taskkill_args(&pid_arg);
        let _ = Command::new("taskkill")
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = child.kill();
    }

    let _ = child.wait();
}

impl Drop for OllamaProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

// ── vllm-mlx ───────────────────────────────────────────────────────────────

/// One vllm-mlx server slot: a (base_url, model) tuple.
///
/// Used by [`VllmMlxProcess::ensure_slots`] to spawn one process per unique
/// slot. The two-slot Apple Silicon loadout uses two slots — large model on
/// :8000 for Dialogue, small model on :8001 for Intent/Reaction/Simulation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VllmMlxSlot {
    /// Base URL including port (e.g. `http://localhost:8001`).
    pub base_url: String,
    /// Hugging Face model id (e.g. `mlx-community/Qwen2.5-1.5B-Instruct-4bit`).
    pub model: String,
}

/// Resolved spawn shape for `VLLM_MLX_BIN`.
///
/// Two layouts are supported:
///
/// 1. **Native binary** — `VLLM_MLX_BIN=/path/to/vllm-mlx`. The binary
///    takes `serve <model> --port N ...` directly. This is what
///    `uv tool install vllm-mlx` produces on a dev machine.
///
/// 2. **Bundled runtime** — `VLLM_MLX_BIN=/path/to/python3` (any path
///    whose filename starts with `python`). The binary is the
///    interpreter inside our bundled portable Python, with the
///    `vllm_mlx` package pip-installed into its own site-packages.
///    The spawn becomes `python3 -m vllm_mlx.cli serve <model> --port
///    N ...`. (`vllm_mlx` itself isn't directly executable as `-m
///    vllm_mlx` — the package has no `__main__.py`; the console
///    entry point installed by pip is `vllm_mlx.cli:main`, which we
///    invoke directly so we don't depend on the pip-generated
///    shebang line — that shebang is baked at install time and would
///    point at the build machine inside a shipped .app.)
///
/// The discriminator is the filename: if the basename starts with
/// `python` we add `-m vllm_mlx.cli` as a prefix; otherwise we
/// invoke the binary directly.
struct VllmMlxInvocation {
    program: String,
    prefix_args: Vec<&'static str>,
}

impl VllmMlxInvocation {
    fn resolve(bin: &str) -> Self {
        let basename = std::path::Path::new(bin)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if basename.starts_with("python") {
            Self {
                program: bin.to_string(),
                prefix_args: vec!["-m", "vllm_mlx.cli"],
            }
        } else {
            Self {
                program: bin.to_string(),
                prefix_args: Vec::new(),
            }
        }
    }
}

/// Manages a vllm-mlx server process started by Parish (macOS / Apple Silicon local runtime).
///
/// Parallels [`OllamaProcess`] for the vllm-mlx runtime: probes
/// `/v1/models` first, spawns `vllm-mlx serve <model> --port <p>
/// --enable-prefix-cache --continuous-batching` if not already
/// running, and stops the child on drop.
///
/// `VLLM_MLX_BIN` overrides the binary path. This works around the
/// case where installing rapid-mlx clobbers the `vllm-mlx` symlink
/// in `~/.local/bin`; setting `VLLM_MLX_BIN=$HOME/.local/share/uv/tools/vllm-mlx/bin/vllm-mlx`
/// keeps Parish on the pristine binary.
///
/// Wired into [`setup_provider_client`] for `Provider::VllmMlx`. The
/// base provider slot is always spawned; additional slots may be
/// supplied via `extra_vllm_slots` for per-category routing
/// (two-slot Apple Silicon loadout).
///
/// [`setup_provider_client`]: crate::orchestration::setup_provider_client
pub struct VllmMlxProcess {
    pub(super) child: Option<Child>,
}

impl VllmMlxProcess {
    /// Creates a no-op process handle (for non-vllm-mlx providers).
    pub fn none() -> Self {
        Self { child: None }
    }

    /// Checks if vllm-mlx is reachable at `base_url`. If not, spawns
    /// `vllm-mlx serve <model> --port <port>` and waits for the
    /// `/v1/models` endpoint to respond (up to 60 s).
    ///
    /// `base_url` should include the port (e.g.
    /// `http://localhost:8000/v1`); the port is parsed back out for
    /// the `--port` flag, falling back to 8000 if absent.
    pub async fn ensure_running(base_url: &str, model_name: &str) -> Result<Self, ParishError> {
        if Self::is_reachable(base_url).await {
            tracing::info!("vllm-mlx already running at {}", base_url);
            return Ok(Self { child: None });
        }

        tracing::info!("vllm-mlx not detected, starting vllm-mlx serve...");

        let port = super::orchestration::port_from_base_url(base_url).unwrap_or(8000);
        let bin = std::env::var("VLLM_MLX_BIN").unwrap_or_else(|_| "vllm-mlx".to_string());
        let invocation = VllmMlxInvocation::resolve(&bin);

        let mut command = Command::new(invocation.program);
        for arg in invocation.prefix_args {
            command.arg(arg);
        }
        command
            .arg("serve")
            .arg(model_name)
            .arg("--port")
            .arg(port.to_string())
            .arg("--enable-prefix-cache")
            .arg("--continuous-batching");

        // Inject HF cache + offline env so a child python -m vllm_mlx reads
        // models from the cache our HfModelDownloader populated and skips
        // any HF round-trips during gameplay. PARISH_HF_HOME and
        // PARISH_VLLM_MLX_PYTHONPATH are set by parish-tauri at startup
        // when a bundled venv is detected; unset for dev runs (PATH
        // fallback expects an already-installed vllm-mlx and its global
        // ~/.cache/huggingface).
        if let Ok(hf_home) = std::env::var("PARISH_HF_HOME") {
            command.env("HF_HOME", hf_home);
            command.env("HF_HUB_OFFLINE", "1");
        }
        if let Ok(pp) = std::env::var("PARISH_VLLM_MLX_PYTHONPATH") {
            command.env("PYTHONPATH", pp);
        }

        let child = command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                ParishError::Inference(format!(
                    "failed to start vllm-mlx at `{}`: {}. \
                     Packaged Parish builds ship vllm-mlx in app resources and \
                     auto-set VLLM_MLX_BIN; if you see this from a packaged \
                     build the bundle is incomplete. For dev builds, install \
                     with `uv tool install vllm-mlx` or set VLLM_MLX_BIN to a \
                     binary path.",
                    bin, e
                ))
            })?;

        // Poll for readiness (up to 60 s; cold-load measured at ~3.3 s
        // when prefix-cache is persisted, longer on first-ever launch).
        let mut ready = false;
        for i in 0..120 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if Self::is_reachable(base_url).await {
                tracing::info!("vllm-mlx ready after ~{}ms", (i + 1) * 500);
                ready = true;
                break;
            }
        }

        if !ready {
            kill_and_reap(child);
            return Err(ParishError::Inference(
                "vllm-mlx serve started but did not become reachable within 60s".to_string(),
            ));
        }

        Ok(Self { child: Some(child) })
    }

    /// Ensures each unique [`VllmMlxSlot`] has a vllm-mlx server reachable.
    ///
    /// Deduplicates slots by `(base_url, model)`. Returns one
    /// [`VllmMlxProcess`] per unique slot — slots that were already running
    /// produce a no-op handle. Caller must hold all handles for the
    /// app lifetime so spawned children are stopped on drop.
    pub async fn ensure_slots(slots: &[VllmMlxSlot]) -> Result<Vec<Self>, ParishError> {
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut out = Vec::with_capacity(slots.len());
        for slot in slots {
            let key = (slot.base_url.clone(), slot.model.clone());
            if !seen.insert(key) {
                continue;
            }
            out.push(Self::ensure_running(&slot.base_url, &slot.model).await?);
        }
        Ok(out)
    }

    /// Returns whether we started the vllm-mlx process (vs. it was already running).
    pub fn was_started_by_us(&self) -> bool {
        self.child.is_some()
    }

    /// Probes `/v1/models` to see if a vllm-mlx server is reachable.
    async fn is_reachable(base_url: &str) -> bool {
        let url = format!("{}/models", base_url.trim_end_matches('/'));
        let client =
            build_client_or_fallback(Duration::from_secs(2), "vllm-mlx reachability probe");
        client.get(&url).send().await.is_ok()
    }

    /// Stops the vllm-mlx process if we started it.
    pub fn stop(&mut self) {
        if let Some(ref mut child) = self.child {
            tracing::info!("Stopping vllm-mlx server...");
            let _ = child.kill();
            let _ = child.wait();
            self.child = None;
        }
    }
}

impl Drop for VllmMlxProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

// ── vllm ───────────────────────────────────────────────────────────────────

/// One vllm server slot (Linux/Windows CUDA/ROCm): a (base_url, model) tuple.
///
/// Parallel to [`VllmMlxSlot`] for the standard vllm runtime.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VllmSlot {
    /// Base URL including port (e.g. `http://localhost:8001`).
    pub base_url: String,
    /// Hugging Face model id (e.g. `Qwen/Qwen2.5-1.5B-Instruct`).
    pub model: String,
}

/// Manages a vllm server process started by Parish (Linux/Windows, CUDA/ROCm).
///
/// Parallels [`VllmMlxProcess`] for the standard vllm runtime: probes
/// `/v1/models` first, spawns `vllm serve <model> --port <p>` if not
/// already running, and stops the child on drop.
///
/// `VLLM_BIN` overrides the binary path (defaults to `vllm`).
pub struct VllmProcess {
    pub(super) child: Option<Child>,
}

impl VllmProcess {
    /// Creates a no-op process handle (for non-vllm providers).
    pub fn none() -> Self {
        Self { child: None }
    }

    /// Checks if vllm is reachable at `base_url`. If not, spawns
    /// `vllm serve <model> --port <port>` and waits for the
    /// `/v1/models` endpoint to respond (up to 60 s).
    pub async fn ensure_running(base_url: &str, model_name: &str) -> Result<Self, ParishError> {
        if Self::is_reachable(base_url).await {
            tracing::info!("vllm already running at {}", base_url);
            return Ok(Self { child: None });
        }

        tracing::info!("vllm not detected, starting vllm serve...");

        let port = super::orchestration::port_from_base_url(base_url).unwrap_or(8000);
        let bin = std::env::var("VLLM_BIN").unwrap_or_else(|_| "vllm".to_string());

        let child = Command::new(&bin)
            .arg("serve")
            .arg(model_name)
            .arg("--port")
            .arg(port.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                ParishError::Inference(format!(
                    "failed to start vllm at `{}`: {}. \
                     Install with `pip install vllm` or set VLLM_BIN to a binary path.",
                    bin, e
                ))
            })?;

        let mut ready = false;
        for i in 0..120 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if Self::is_reachable(base_url).await {
                tracing::info!("vllm ready after ~{}ms", (i + 1) * 500);
                ready = true;
                break;
            }
        }

        if !ready {
            kill_and_reap(child);
            return Err(ParishError::Inference(
                "vllm serve started but did not become reachable within 60s".to_string(),
            ));
        }

        Ok(Self { child: Some(child) })
    }

    /// Ensures each unique [`VllmSlot`] has a vllm server reachable.
    pub async fn ensure_slots(slots: &[VllmSlot]) -> Result<Vec<Self>, ParishError> {
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut out = Vec::with_capacity(slots.len());
        for slot in slots {
            let key = (slot.base_url.clone(), slot.model.clone());
            if !seen.insert(key) {
                continue;
            }
            out.push(Self::ensure_running(&slot.base_url, &slot.model).await?);
        }
        Ok(out)
    }

    /// Returns whether we started the vllm process (vs. it was already running).
    pub fn was_started_by_us(&self) -> bool {
        self.child.is_some()
    }

    async fn is_reachable(base_url: &str) -> bool {
        let url = format!("{}/models", base_url.trim_end_matches('/'));
        let client = build_client_or_fallback(Duration::from_secs(2), "vllm reachability probe");
        client.get(&url).send().await.is_ok()
    }

    /// Stops the vllm process if we started it.
    pub fn stop(&mut self) {
        if let Some(ref mut child) = self.child {
            tracing::info!("Stopping vllm server...");
            let _ = child.kill();
            let _ = child.wait();
            self.child = None;
        }
    }
}

impl Drop for VllmProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

// ── RuntimeProcesses bundle ────────────────────────────────────────────────

/// Bundle of runtime processes started by Parish during provider setup.
///
/// Carries either an [`OllamaProcess`] (when `Provider::Ollama` is the base)
/// or a [`Vec<VllmMlxProcess>`] / [`Vec<VllmProcess>`] (when the matching
/// local provider is the base or per-category routing spawns slots).
/// Callers must hold this value for the app lifetime so children are
/// stopped on drop.
pub struct RuntimeProcesses {
    /// Ollama child process, if Ollama is the base provider.
    pub ollama: OllamaProcess,
    /// One vllm-mlx process per unique slot (base + per-category overrides).
    pub vllm_mlx: Vec<VllmMlxProcess>,
    /// One vllm process per unique slot (base + per-category overrides).
    pub vllm: Vec<VllmProcess>,
}

impl RuntimeProcesses {
    /// Creates an empty bundle (no spawned processes).
    pub fn none() -> Self {
        Self {
            ollama: OllamaProcess::none(),
            vllm_mlx: Vec::new(),
            vllm: Vec::new(),
        }
    }

    /// Stops every child process (Ollama + each vllm-mlx / vllm slot).
    ///
    /// Safe to call multiple times; each underlying process tracks its own
    /// `Option<Child>` and skips when already stopped. Drop impls also call
    /// `stop()`, so explicit calls are only needed when callers want to
    /// release resources before the bundle goes out of scope.
    pub fn stop(&mut self) {
        self.ollama.stop();
        for slot in &mut self.vllm_mlx {
            slot.stop();
        }
        for slot in &mut self.vllm {
            slot.stop();
        }
    }
}

impl Default for RuntimeProcesses {
    fn default() -> Self {
        Self::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vllm_mlx_process_none_is_no_op() {
        let mut p = VllmMlxProcess::none();
        assert!(!p.was_started_by_us());
        p.stop(); // must not panic on no-op
    }

    // ── VllmMlxInvocation: native vs python-venv spawn shapes ──────────────

    #[test]
    fn invocation_resolves_native_binary_directly() {
        // Dev path: `uv tool install vllm-mlx` puts a wrapper at
        // ~/.local/bin/vllm-mlx. Spawn calls it with `serve` as the first arg.
        let inv = VllmMlxInvocation::resolve("/Users/dev/.local/bin/vllm-mlx");
        assert_eq!(inv.program, "/Users/dev/.local/bin/vllm-mlx");
        assert!(inv.prefix_args.is_empty());
    }

    #[test]
    fn invocation_resolves_python_to_module_invocation() {
        // Packaged path: bundle ships a portable Python and the vllm_mlx
        // package on PYTHONPATH. Spawn calls `python3 -m vllm_mlx serve …`.
        let inv = VllmMlxInvocation::resolve(
            "/Applications/Rundale.app/Contents/Resources/vllm-mlx/python-runtime/bin/python3",
        );
        assert!(inv.program.ends_with("python3"));
        assert_eq!(inv.prefix_args, vec!["-m", "vllm_mlx.cli"]);
    }

    #[test]
    fn invocation_treats_python3_13_as_python() {
        // Versioned interpreter names (`python3.13`, `python3.14`) must
        // still trigger the `-m vllm_mlx` path.
        let inv = VllmMlxInvocation::resolve("/opt/parish/python-runtime/bin/python3.13");
        assert_eq!(inv.prefix_args, vec!["-m", "vllm_mlx.cli"]);
    }

    #[test]
    fn invocation_falls_back_to_native_on_bare_name() {
        // Production env on dev Macs: `VLLM_MLX_BIN=vllm-mlx` (no path).
        let inv = VllmMlxInvocation::resolve("vllm-mlx");
        assert!(inv.prefix_args.is_empty());
    }

    #[test]
    fn test_runtime_processes_none_is_no_op() {
        let mut p = RuntimeProcesses::none();
        assert!(p.vllm_mlx.is_empty());
        assert!(p.vllm.is_empty());
        p.stop(); // ollama no-op + vllm_mlx empty + vllm empty must not panic
    }

    #[test]
    fn test_runtime_processes_default_matches_none() {
        let p = RuntimeProcesses::default();
        assert!(p.vllm_mlx.is_empty());
        assert!(p.vllm.is_empty());
    }

    #[test]
    fn test_vllm_process_none_is_no_op() {
        let mut p = VllmProcess::none();
        assert!(!p.was_started_by_us());
        p.stop();
    }

    #[test]
    fn test_vllm_slot_eq_and_hash() {
        use std::collections::HashSet;
        let a = VllmSlot {
            base_url: "http://localhost:8001".to_string(),
            model: "Qwen/Qwen2.5-1.5B-Instruct".to_string(),
        };
        let b = a.clone();
        let c = VllmSlot {
            base_url: "http://localhost:8000".to_string(),
            model: "Qwen/Qwen2.5-14B-Instruct".to_string(),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
        let set: HashSet<_> = [a.clone(), b.clone(), c.clone()].iter().cloned().collect();
        assert_eq!(set.len(), 2, "duplicate slot must dedup in the HashSet");
    }

    #[tokio::test]
    async fn test_vllm_process_ensure_slots_empty_input() {
        // No slots → no spawns, no errors.
        let out = VllmProcess::ensure_slots(&[]).await.unwrap();
        assert!(out.is_empty());
    }

    /// Regression guard for the two-slot loadout: `ensure_slots` must spawn
    /// exactly one process per unique `(base_url, model)` tuple. If two
    /// category overrides point to the same slot, only one server is spawned.
    ///
    /// We can't reach the network here, but the slot-builder is pure and the
    /// dedup happens before any spawn — verify via the dedup-input contract.
    #[test]
    fn test_vllm_mlx_slot_eq_and_hash() {
        use std::collections::HashSet;
        let a = VllmMlxSlot {
            base_url: "http://localhost:8001".to_string(),
            model: "mlx-community/Qwen2.5-1.5B-Instruct-4bit".to_string(),
        };
        let b = a.clone();
        let c = VllmMlxSlot {
            base_url: "http://localhost:8000".to_string(),
            model: "mlx-community/Qwen2.5-7B-Instruct-4bit".to_string(),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
        let set: HashSet<_> = [a.clone(), b.clone(), c.clone()].iter().cloned().collect();
        assert_eq!(set.len(), 2, "duplicate slot must dedup in the HashSet");
    }

    /// Regression guard for TD-015 — Windows `taskkill` argument vector.
    ///
    /// `OllamaProcess::stop` on Windows force-kills the entire process tree so
    /// orphan GPU workers release VRAM. Drift in this argv (e.g. dropping `/T`
    /// or `/F`) would silently leak GPU memory across restarts. The Command
    /// invocation itself is platform-locked, but the argv is pure and tested
    /// here on every host.
    #[test]
    fn taskkill_args_are_force_tree_kill_with_pid() {
        let pid = pid_string(4242);
        assert_eq!(pid, "4242");
        assert_eq!(taskkill_args(&pid), ["/F", "/T", "/PID", "4242"]);
    }

    /// PIDs at the u32 boundary must still format without panicking — a defensive
    /// check since Windows can recycle PIDs into the upper range under load.
    #[test]
    fn taskkill_args_handle_u32_max_pid() {
        let pid = pid_string(u32::MAX);
        assert_eq!(pid, "4294967295");
        assert_eq!(taskkill_args(&pid), ["/F", "/T", "/PID", "4294967295"]);
    }
}
