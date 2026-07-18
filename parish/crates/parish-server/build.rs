//! Build script for `parish-server`.
//!
//! Scans `apps/ui/dist/**/*.html` for inline `<script>` blocks, computes their
//! SHA-256 digests, and writes a generated Rust source file with the hash list.
//! The server's Content-Security-Policy includes these hashes in `script-src`
//! instead of the weaker `'unsafe-inline'`.
//!
//! If no `dist/` directory is found (e.g. on a fresh checkout before `npm run
//! build`), the script emits an empty hash slice and a compile-time warning so
//! the server still compiles, but the resulting CSP will reject all inline
//! scripts.  Run `just ui-build` (or `npm run build` inside `apps/ui/`) before
//! `cargo build` to get a correct hash list.
//!
//! Tests may set `PARISH_UI_DIST_DIR` to supply an isolated generated HTML
//! tree; Cargo rebuilds this script when that override changes.

use std::collections::BTreeSet;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

const UI_DIST_DIR_ENV: &str = "PARISH_UI_DIST_DIR";

fn main() {
    println!("cargo:rerun-if-env-changed={UI_DIST_DIR_ENV}");
    // Playwright's managed-server helper sets these to the invoking worktree's
    // UI digest and expected identity. Shared Cargo targets must not reuse a
    // build-script result belonging to a different dist snapshot.
    println!("cargo:rerun-if-env-changed=PARISH_UI_DIST_DIGEST");
    println!("cargo:rerun-if-env-changed=PARISH_PLAYWRIGHT_BUILD_ID");

    // Retain the dist root as a catch-all for a newly generated route, then
    // emit one concrete HTML path below. Cargo does not reliably invalidate a
    // recursive directory watch when a nested Vite page's contents change.
    let dist = ui_dist_dir();
    println!("cargo:rerun-if-changed={}", dist.display());

    let hashes = if dist.exists() {
        collect_script_hashes(&dist)
    } else {
        // No dist directory — warn but don't fail.
        println!(
            "cargo:warning=parish-server build.rs: {} not found; \
             script-src hashes will be empty. Run `just ui-build` first.",
            dist.display()
        );
        BTreeSet::new()
    };

    let playwright_build_id = std::env::var("PARISH_PLAYWRIGHT_BUILD_ID").ok();
    write_generated_file(&hashes, playwright_build_id.as_deref());
}

/// Resolve the generated UI directory, with a test-friendly override for
/// isolated Cargo builds. The production default is `apps/ui/dist`.
fn ui_dist_dir() -> PathBuf {
    std::env::var_os(UI_DIST_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../apps/ui/dist"))
}

/// Walk all `*.html` files under `dir`, extract every inline `<script>` block's
/// content, compute its SHA-256 digest, and return the unique set of
/// `'sha256-<base64>'` tokens.
fn collect_script_hashes(dir: &Path) -> BTreeSet<String> {
    use sha2::{Digest, Sha256};

    let mut hashes = BTreeSet::new();

    for entry in walkdir(dir) {
        let path = match entry {
            Ok(p) => p,
            Err(e) => {
                eprintln!("cargo:warning=parish-server build.rs: walk error: {e}");
                continue;
            }
        };
        if path.extension().and_then(|e| e.to_str()) != Some("html") {
            continue;
        }

        // Vite emits routes recursively (for example, `editor/index.html`).
        // Watch each generated HTML file so a changed inline bootstrap causes
        // Cargo to regenerate the CSP hash list on the next server build.
        println!("cargo:rerun-if-changed={}", path.display());

        let html = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "cargo:warning=parish-server build.rs: cannot read {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        for script_content in extract_inline_scripts(&html) {
            let digest = Sha256::digest(script_content.as_bytes());
            let b64 = base64_encode(digest.as_slice());
            hashes.insert(format!("'sha256-{b64}'"));
        }
    }

    hashes
}

/// Walk a directory tree, yielding `Result<PathBuf, _>` for each file.
/// This is a minimal stand-in for the `walkdir` crate (which is not available
/// as a build dependency here) using only `std::fs::read_dir`.
fn walkdir(root: &Path) -> impl Iterator<Item = Result<std::path::PathBuf, std::io::Error>> {
    let mut stack: Vec<std::path::PathBuf> = vec![root.to_path_buf()];
    let mut files: Vec<Result<std::path::PathBuf, std::io::Error>> = Vec::new();

    while let Some(dir) = stack.pop() {
        match std::fs::read_dir(&dir) {
            Err(e) => files.push(Err(e)),
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Err(e) => files.push(Err(e)),
                        Ok(e) => {
                            let path = e.path();
                            if path.is_dir() {
                                stack.push(path);
                            } else {
                                files.push(Ok(path));
                            }
                        }
                    }
                }
            }
        }
    }

    files.into_iter()
}

/// Extract the text content of every `<script>…</script>` block that has no
/// attributes (i.e. plain inline scripts, not `<script src="…">`).
///
/// SvelteKit's static adapter emits exactly one such block per page.
fn extract_inline_scripts(html: &str) -> Vec<&str> {
    let mut scripts = Vec::new();
    let mut remaining = html;

    // We deliberately avoid a regex crate to keep build dependencies minimal.
    // The pattern is simple: find `<script>`, then `</script>`.
    while let Some(start_tag) = find_script_open_tag(remaining) {
        let after_open = &remaining[start_tag..];
        // Find the `>` that closes the opening tag.
        if let Some(gt) = after_open.find('>') {
            let content_start = gt + 1;
            let content_and_rest = &after_open[content_start..];
            if let Some(end) = content_and_rest.find("</script>") {
                scripts.push(&content_and_rest[..end]);
                // Advance past the closing tag.
                remaining = &content_and_rest[end + "</script>".len()..];
            } else {
                break;
            }
        } else {
            break;
        }
    }

    scripts
}

/// Find the byte offset (within `html`) of a `<script>` opening tag that has
/// **no** attributes — i.e. the tag is exactly `<script>` (possibly
/// mixed-case).  Returns `None` if no such tag is found.
///
/// We skip tags that have attributes (`<script src=…>`, `<script type=…>`,
/// etc.) because those load external scripts and are not inline.
fn find_script_open_tag(html: &str) -> Option<usize> {
    let mut pos = 0;
    while pos < html.len() {
        let remaining = &html[pos..];
        // Look for any `<script` (case-insensitive search using lowercase).
        let lower = remaining.to_ascii_lowercase();
        let rel = lower.find("<script")?;

        // Check whether the character immediately after `<script` is `>` or
        // whitespace then `>` (no attributes), OR is a letter/`=`/`"` which
        // indicates an attribute (skip it).
        let after_keyword = rel + "<script".len();
        let next_char = lower.as_bytes().get(after_keyword).copied();
        match next_char {
            Some(b'>') | Some(b'\n') | Some(b'\r') | Some(b'\t') | Some(b' ') => {
                // Might be an attribute-free tag.  Look at the full slice up to
                // the closing `>` and verify no `=` is present (which would
                // indicate an attribute like `type=` or `src=`).
                let tag_rest = &lower[after_keyword..];
                if let Some(gt_rel) = tag_rest.find('>') {
                    let between = &tag_rest[..gt_rel];
                    if !between.contains('=') && !between.contains("src") {
                        return Some(pos + rel);
                    }
                }
            }
            _ => {}
        }

        pos += rel + 1;
    }
    None
}

/// Minimal base64 encoder (standard alphabet with padding).
/// Avoids pulling in the `base64` crate as a build-dep while keeping the
/// implementation trivial and auditable.
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Write the generated Rust source file to `$OUT_DIR/csp_script_hashes.rs`.
fn write_generated_file(hashes: &BTreeSet<String>, playwright_build_id: Option<&str>) {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest = Path::new(&out_dir).join("csp_script_hashes.rs");
    let mut file = std::fs::File::create(&dest).expect("cannot create csp_script_hashes.rs");

    writeln!(
        file,
        "// Auto-generated by build.rs — do not edit.
// SHA-256 hashes of every inline <script> block in apps/ui/dist/**/*.html.
// Included into src/lib.rs via include!().
/// Hashes of the SvelteKit bootstrap inline scripts, computed at build time.
/// Each entry is a CSP `'sha256-<base64>'` token ready for insertion into
/// `script-src`.
pub const SCRIPT_SRC_HASHES: &[&str] = &["
    )
    .expect("write error");

    for hash in hashes {
        // Escape any special chars just in case (there should be none in a
        // base64 string, but be explicit).
        writeln!(file, "    \"{hash}\",").expect("write error");
    }

    writeln!(file, "];").expect("write error");
    writeln!(
        file,
        "/// Worktree/UI identity embedded by the Playwright managed-server build.\n\
         pub const PLAYWRIGHT_BUILD_ID: Option<&str> = {playwright_build_id:?};"
    )
    .expect("write error");
}

// ── Unit tests for the build-script helpers ───────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encode_hello() {
        // RFC 4648 test vectors.
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn extract_inline_scripts_finds_sveltekit_bootstrap() {
        let html = r#"<!doctype html>
<html>
<head></head>
<body>
<script>
{
    __sveltekit_abc = { base: "" };
    const element = document.currentScript.parentElement;
    Promise.all([import("/_app/start.js")]).then(([k]) => k.start());
}
</script>
</body>
</html>"#;
        let scripts = extract_inline_scripts(html);
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].contains("__sveltekit_abc"));
    }

    #[test]
    fn extract_inline_scripts_skips_external_src() {
        let html = r#"<script src="/foo.js"></script><script>inline</script>"#;
        let scripts = extract_inline_scripts(html);
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], "inline");
    }

    #[test]
    fn find_script_open_tag_skips_typed() {
        // <script type="module"> should be skipped (has an attribute).
        let html = r#"<script type="module">var x = 1;</script><script>inline</script>"#;
        // We only look for the first match in find_script_open_tag.
        // extract_inline_scripts uses it in a loop so both are attempted.
        let scripts = extract_inline_scripts(html);
        // Only the plain <script> should match.
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0], "inline");
    }
}
