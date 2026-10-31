//! Private, line-delimited protocol for the native session host.
use crate::{
    backend::{BackendError, TauriBackend},
    tools::ToolDef,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

const MAX_LINE: u64 = 4 * 1024 * 1024;
struct Process {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    broken: bool,
}
pub struct SessionHostBackend {
    process: Mutex<Process>,
}

impl SessionHostBackend {
    pub async fn spawn(
        executable: PathBuf,
        mod_dir: PathBuf,
        save_dir: PathBuf,
        inference: &str,
        fixture: Option<PathBuf>,
    ) -> Result<Self, BackendError> {
        if !mod_dir.is_absolute()
            || !save_dir.is_absolute()
            || fixture.as_ref().is_some_and(|p| !p.is_absolute())
        {
            return Err(BackendError::Transport(
                "mod, save, and fixture paths must be absolute".into(),
            ));
        }
        let mut command = Command::new(executable);
        command
            .arg("--protocol")
            .arg("--mod-dir")
            .arg(mod_dir)
            .arg("--save-dir")
            .arg(save_dir)
            .arg("--inference")
            .arg(inference);
        if let Some(path) = fixture {
            command.arg("--sse-fixture").arg(path);
        }
        Self::spawn_command(command).await
    }

    async fn spawn_command(mut command: Command) -> Result<Self, BackendError> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| BackendError::Transport(format!("cannot spawn session host: {e}")))?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        let backend = Self {
            process: Mutex::new(Process {
                child,
                stdin,
                stdout,
                next_id: 1,
                broken: false,
            }),
        };
        let result = backend
            .request("initialize", json!({}), Duration::from_secs(10))
            .await?;
        if result.get("protocolVersion").and_then(Value::as_u64) != Some(1) {
            let mut p = backend.process.lock().await;
            p.break_process().await;
            return Err(BackendError::Transport(
                "invalid session host handshake".into(),
            ));
        }
        Ok(backend)
    }
    async fn request(
        &self,
        command: &str,
        args: Value,
        timeout: Duration,
    ) -> Result<Value, BackendError> {
        let mut p = self.process.lock().await;
        if p.broken {
            return Err(BackendError::Transport("session host unavailable".into()));
        }
        let id = p.next_id;
        p.next_id += 1;
        let request = json!({"version":1,"id":id,"command":command,"args":args});
        let mut bytes = serde_json::to_vec(&request)
            .map_err(|_| BackendError::Transport("invalid request".into()))?;
        bytes.push(b'\n');
        let exchange = async {
            p.stdin.write_all(&bytes).await.map_err(|_| ())?;
            p.stdin.flush().await.map_err(|_| ())?;
            let mut line = Vec::new();
            // Limit allocation even when the child never sends a newline.
            let n = (&mut p.stdout)
                .take(MAX_LINE + 1)
                .read_until(b'\n', &mut line)
                .await
                .map_err(|_| ())?;
            if n == 0 || n as u64 > MAX_LINE || line.last() != Some(&b'\n') {
                return Err(());
            }
            let response: Value = serde_json::from_slice(&line).map_err(|_| ())?;
            if response.get("version").and_then(Value::as_u64) != Some(1)
                || response.get("id").and_then(Value::as_u64) != Some(id)
            {
                return Err(());
            }
            match (response.get("result"), response.get("error")) {
                (Some(result), None) if result.is_object() => Ok(Ok(result.clone())),
                (None, Some(error))
                    if error.get("code").and_then(Value::as_str).is_some()
                        && error.get("message").and_then(Value::as_str).is_some() =>
                {
                    Ok(Err(BackendError::Rejected(format!(
                        "{}: {}",
                        error["code"].as_str().unwrap(),
                        error["message"].as_str().unwrap()
                    ))))
                }
                _ => Err(()),
            }
        };
        match tokio::time::timeout(timeout, exchange).await {
            Ok(Ok(result)) => result,
            _ => {
                p.break_process().await;
                Err(BackendError::Transport(
                    "session host protocol failed or timed out".into(),
                ))
            }
        }
    }
}
impl Process {
    async fn break_process(&mut self) {
        self.broken = true;
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

macro_rules! translate {
    ($name:ident, $cmd:literal) => {
        fn $name(args: &Value) -> Result<(String, Value), String> {
            Ok((
                $cmd.into(),
                if args.is_null() {
                    json!({})
                } else if args.is_object() {
                    args.clone()
                } else {
                    return Err("arguments must be an object".into());
                },
            ))
        }
    };
}
fn new_game(args: &Value) -> Result<(String, Value), String> {
    if args.is_null() {
        return Ok(("new_game".into(), json!({})));
    }
    if !args.is_object() {
        return Err("arguments must be an object".into());
    }
    if args
        .get("replaceExisting")
        .is_some_and(|value| !value.is_boolean())
    {
        return Err("`replaceExisting` must be a boolean".into());
    }
    Ok(("new_game".into(), args.clone()))
}
translate!(resume_session, "resume_session");
fn submit_input(args: &Value) -> Result<(String, Value), String> {
    required_strings(args, "submit_input", &["text"])
}
fn answer_clarification(args: &Value) -> Result<(String, Value), String> {
    required_strings(args, "answer_clarification", &["questionID", "choiceID"])
}
fn required_strings(args: &Value, command: &str, keys: &[&str]) -> Result<(String, Value), String> {
    if !args.is_object() {
        return Err("arguments must be an object".into());
    }
    for key in keys {
        if args.get(*key).and_then(Value::as_str).is_none() {
            return Err(format!("missing required string field: `{key}`"));
        }
    }
    Ok((command.into(), args.clone()))
}
translate!(stop, "stop");
translate!(retry, "retry");
translate!(persist_session, "persist_session");
translate!(session_presentation, "session_presentation");
translate!(close_session, "close_session");
fn tools() -> Vec<ToolDef> {
    let defs: [(&str, &str, Value, crate::tools::TranslateFn); 9] = [
        (
            "limerick_new_game",
            "Start a new game.",
            json!({"type":"object","properties":{"replaceExisting":{"type":"boolean","default":false}}}),
            new_game,
        ),
        (
            "limerick_resume_session",
            "Resume the saved session.",
            json!({"type":"object","properties":{}}),
            resume_session,
        ),
        (
            "limerick_submit_input",
            "Submit player input.",
            json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}),
            submit_input,
        ),
        (
            "limerick_answer_clarification",
            "Answer a clarification.",
            json!({"type":"object","properties":{"questionID":{"type":"string"},"choiceID":{"type":"string"}},"required":["questionID","choiceID"]}),
            answer_clarification,
        ),
        (
            "limerick_stop",
            "Stop the current turn.",
            json!({"type":"object","properties":{}}),
            stop,
        ),
        (
            "limerick_retry",
            "Retry the current turn.",
            json!({"type":"object","properties":{}}),
            retry,
        ),
        (
            "limerick_persist_session",
            "Save the session.",
            json!({"type":"object","properties":{}}),
            persist_session,
        ),
        (
            "limerick_session_presentation",
            "Read the current session presentation.",
            json!({"type":"object","properties":{}}),
            session_presentation,
        ),
        (
            "limerick_close_session",
            "Close the session.",
            json!({"type":"object","properties":{}}),
            close_session,
        ),
    ];
    defs.into_iter()
        .map(|(name, description, input_schema, translate)| ToolDef {
            name,
            description,
            input_schema,
            translate,
        })
        .collect()
}
#[async_trait]
impl TauriBackend for SessionHostBackend {
    fn name(&self) -> &'static str {
        "session-host"
    }
    fn tools(&self) -> Vec<ToolDef> {
        tools()
    }
    fn structured_results(&self) -> bool {
        true
    }
    async fn invoke(&self, command: &str, args: Value) -> Result<Value, BackendError> {
        let result = self
            .request(
                command,
                args,
                Duration::from_secs(if matches!(command, "new_game" | "resume_session") {
                    60
                } else {
                    10
                }),
            )
            .await?;
        if !result.get("markdown").is_some_and(Value::is_string)
            || !result.get("presentation").is_some_and(Value::is_object)
            || !result.get("history").is_some_and(Value::is_array)
        {
            self.process.lock().await.break_process().await;
            return Err(BackendError::Transport(
                "invalid session host result".into(),
            ));
        }
        Ok(result)
    }
    async fn shutdown(&self) {
        self.shutdown_with_timeout(Duration::from_secs(10)).await;
    }
}
impl SessionHostBackend {
    async fn shutdown_with_timeout(&self, timeout: Duration) {
        let graceful = async {
            self.request("shutdown", json!({}), timeout).await?;
            let mut p = self.process.lock().await;
            p.child
                .wait()
                .await
                .map_err(|_| BackendError::Transport("session host wait failed".into()))?;
            Ok::<(), BackendError>(())
        };
        if !matches!(tokio::time::timeout(timeout, graceful).await, Ok(Ok(()))) {
            self.process.lock().await.break_process().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicU64, Ordering},
    };
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    fn fixture(mode: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "mcp-host-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("host");
        std::fs::write(&script, format!(r##"#!/usr/bin/python3
import json, sys, time
for line in sys.stdin:
    req = json.loads(line)
    cmd = req['command']
    if cmd != 'initialize' and '{mode}' == 'malformed':
        print('not json', flush=True); continue
    if cmd != 'initialize' and '{mode}' == 'timeout':
        time.sleep(5); continue
    if cmd != 'initialize' and '{mode}' == 'exit': sys.exit(0)
    if cmd != 'initialize' and '{mode}' == 'oversized':
        print('x' * (4 * 1024 * 1024 + 1), flush=True); continue
    if cmd != 'initialize' and '{mode}' == 'version':
        print(json.dumps({{'version': 2, 'id': req['id'], 'result': {{}}}}), flush=True); continue
    if cmd != 'initialize' and '{mode}' == 'rejected' and cmd == 'stop':
        print(json.dumps({{'version': 1, 'id': req['id'], 'error': {{'code': 'no_session', 'message': 'No session'}}}}), flush=True); continue
    if cmd != 'initialize' and '{mode}' == 'bad_result':
        print(json.dumps({{'version': 1, 'id': req['id'], 'result': {{'markdown': 2}}}}), flush=True); continue
    if cmd == 'shutdown' and '{mode}' == 'hang_shutdown': time.sleep(5)
    if cmd == 'initialize' and '{mode}' == 'bad_handshake':
        print(json.dumps({{'version': 1, 'id': req['id'], 'result': {{'protocolVersion': 2}}}}), flush=True); continue
    result = {{'protocolVersion': 1}} if cmd == 'initialize' else {{'markdown': cmd, 'presentation': {{}}, 'history': []}}
    print(json.dumps({{'version': 1, 'id': req['id'], 'result': result}}), flush=True)
    if cmd == 'shutdown': break
"##)).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, script)
    }
    async fn host(mode: &str) -> (PathBuf, SessionHostBackend) {
        let (dir, script) = fixture(mode);
        let mut command = Command::new("/usr/bin/python3");
        command
            .arg(script)
            .arg("--protocol")
            .arg("--mod-dir")
            .arg(&dir)
            .arg("--save-dir")
            .arg(&dir)
            .arg("--inference")
            .arg("mock");
        let backend = SessionHostBackend::spawn_command(command).await.unwrap();
        (dir, backend)
    }
    #[tokio::test]
    async fn handshake_registry_and_shutdown() {
        let (dir, backend) = host("ok").await;
        assert_eq!(backend.tools().len(), 9);
        assert!(backend.structured_results());
        use crate::jsonrpc::MethodHandler;
        let backend = std::sync::Arc::new(backend);
        let server = crate::mcp::McpServer::new(backend.clone());
        let response = server
            .handle(
                "tools/call",
                json!({"name":"limerick_submit_input","arguments":{"text":"hello"}}),
            )
            .await
            .unwrap();
        assert_eq!(response["content"][0]["text"], "submit_input");
        assert_eq!(response["structuredContent"]["presentation"], json!({}));
        assert_eq!(
            backend
                .invoke("submit_input", json!({"text":"hello"}))
                .await
                .unwrap()["markdown"],
            "submit_input"
        );
        backend.shutdown().await;
        assert!(
            backend
                .process
                .lock()
                .await
                .child
                .try_wait()
                .unwrap()
                .is_some()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn malformed_exit_and_timeout_break_and_reap_child() {
        for mode in ["malformed", "exit", "timeout", "version", "oversized"] {
            let (dir, backend) = host(mode).await;
            assert!(
                backend
                    .request("stop", json!({}), Duration::from_millis(100))
                    .await
                    .is_err()
            );
            let mut p = backend.process.lock().await;
            assert!(p.broken);
            assert!(p.child.try_wait().unwrap().is_some());
            drop(p);
            assert!(backend.invoke("stop", json!({})).await.is_err());
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[tokio::test]
    async fn rejection_keeps_child_usable_and_bad_result_breaks_it() {
        let (dir, backend) = host("rejected").await;
        assert!(matches!(
            backend.invoke("stop", json!({})).await,
            Err(BackendError::Rejected(_))
        ));
        assert!(backend.invoke("retry", json!({})).await.is_ok());
        backend.shutdown().await;
        std::fs::remove_dir_all(dir).unwrap();
        let (dir, backend) = host("bad_result").await;
        assert!(backend.invoke("stop", json!({})).await.is_err());
        assert!(backend.process.lock().await.broken);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn startup_failures_and_bounded_shutdown() {
        let (dir, script) = fixture("bad_handshake");
        let mut command = Command::new("/usr/bin/python3");
        command
            .arg(script)
            .arg("--protocol")
            .arg("--mod-dir")
            .arg(&dir)
            .arg("--save-dir")
            .arg(&dir)
            .arg("--inference")
            .arg("mock");
        assert!(SessionHostBackend::spawn_command(command).await.is_err());
        assert!(
            SessionHostBackend::spawn(dir.join("missing"), dir.clone(), dir.clone(), "mock", None)
                .await
                .is_err()
        );
        std::fs::remove_dir_all(dir).unwrap();
        let (dir, backend) = host("hang_shutdown").await;
        let start = std::time::Instant::now();
        backend
            .shutdown_with_timeout(Duration::from_millis(100))
            .await;
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(
            backend
                .process
                .lock()
                .await
                .child
                .try_wait()
                .unwrap()
                .is_some()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn tool_arguments_enforce_declared_types() {
        assert!(new_game(&json!({"replaceExisting":"yes"})).is_err());
        assert!(new_game(&json!({"replaceExisting":false})).is_ok());
        assert!(submit_input(&json!({"text": 1})).is_err());
        assert!(answer_clarification(&json!({"questionID":"q", "choiceID":false})).is_err());
        assert!(stop(&json!([])).is_err());
    }
}
