use base64::{Engine as _, engine::general_purpose::STANDARD};
use serial_test::serial;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct RunningServer(Child);

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[serial]
fn rebuilt_server_serves_only_the_new_hash_for_nested_vite_bootstrap() {
    let parish_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve Parish workspace directory");
    let target_dir = TempDir::new().expect("create isolated Cargo target directory");
    let dist_dir = target_dir.path().join("ui-dist");
    let html_path = dist_dir.join("editor/index.html");
    fs::create_dir_all(html_path.parent().expect("nested fixture parent"))
        .expect("create nested Vite fixture directory");
    let original_html = "<!doctype html><script>window.__vite_bootstrap = 1;</script>";
    fs::write(&html_path, original_html).expect("write nested Vite bootstrap fixture");

    let original_bootstrap = inline_bootstrap(original_html);
    let old_hash = csp_hash(original_bootstrap);

    let first_binary = build_server(&parish_dir, target_dir.path(), &dist_dir);
    let first_csp = serve_csp(&first_binary, &parish_dir, target_dir.path());
    assert!(
        first_csp.contains(&old_hash),
        "initial server CSP must include the original nested bootstrap hash: {first_csp}"
    );

    let updated_html = original_html.replacen(" = 1;", " = 2;", 1);
    assert_ne!(
        updated_html, original_html,
        "fixture must change Vite bootstrap content"
    );
    fs::write(&html_path, &updated_html).expect("change nested Vite bootstrap");

    let new_hash = csp_hash(inline_bootstrap(&updated_html));
    assert_ne!(
        new_hash, old_hash,
        "changed bootstrap must have a new CSP hash"
    );

    let rebuilt_binary = build_server(&parish_dir, target_dir.path(), &dist_dir);
    let rebuilt_csp = serve_csp(&rebuilt_binary, &parish_dir, target_dir.path());
    assert!(
        rebuilt_csp.contains(&new_hash),
        "rebuilt server CSP must include the changed nested bootstrap hash: {rebuilt_csp}"
    );
    assert!(
        !rebuilt_csp.contains(&old_hash),
        "rebuilt server CSP must not retain the stale nested bootstrap hash: {rebuilt_csp}"
    );
}

fn build_server(parish_dir: &Path, target_dir: &Path, dist_dir: &Path) -> PathBuf {
    let status = Command::new("cargo")
        .current_dir(parish_dir)
        .env("CARGO_TARGET_DIR", target_dir)
        .env("PARISH_UI_DIST_DIR", dist_dir)
        .args(["build", "-p", "parish-server", "--quiet"])
        .status()
        .expect("run cargo build for parish-server");
    assert!(
        status.success(),
        "build parish-server with current Vite dist"
    );
    target_dir.join("debug/parish-server")
}

fn serve_csp(binary: &Path, parish_dir: &Path, target_dir: &Path) -> String {
    let port = available_port();
    let user_data_dir = target_dir.join(format!("user-data-{port}"));
    let server = RunningServer(
        Command::new(binary)
            .current_dir(parish_dir)
            .env("PARISH_USER_DATA_DIR", user_data_dir)
            .args(["--port", &port.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start rebuilt parish-server"),
    );

    let csp = wait_for_csp(port);
    drop(server);
    csp
}

fn available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve a local port");
    listener.local_addr().expect("read reserved port").port()
}

fn wait_for_csp(port: u16) -> String {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(csp) = request_csp(port) {
            return csp;
        }
        assert!(
            Instant::now() < deadline,
            "parish-server did not become ready while checking its CSP"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

fn request_csp(port: u16) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.write_all(
        b"GET /api/world-snapshot HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    response
        .lines()
        .find_map(|line| line.strip_prefix("content-security-policy: "))
        .map(str::to_owned)
        .ok_or_else(|| std::io::Error::other("response omitted Content-Security-Policy"))
}

fn inline_bootstrap(html: &str) -> &str {
    let (_, after_open) = html
        .split_once("<script>")
        .expect("Vite page must contain a plain inline bootstrap script");
    let (bootstrap, _) = after_open
        .split_once("</script>")
        .expect("Vite bootstrap script must have a closing tag");
    bootstrap
}

fn csp_hash(script: &str) -> String {
    let digest = Sha256::digest(script.as_bytes());
    format!("'sha256-{}'", STANDARD.encode(digest))
}
