//! Callback-free C boundary between the native iPhone client and the shared
//! Limerick engine.
//!
//! The ABI carries only borrowed UTF-8 request bytes, owned JSON response
//! bytes, and an opaque session handle (see `include/limerick_mobile_ffi.h`).
//! Every response is a JSON envelope: `{"ok": true, "value": ...}` or
//! `{"ok": false, "error": {"code": ..., "message": ...}}`.
//!
//! A session runs the shared turn API (`limerick_core::turn::TurnEngine`)
//! over the save's journal; see [`session`]. The Swift host submits player
//! input, fulfils each pending model call through Limerick Endpoints, and
//! resumes the engine with the result, a failure, or Stop. The operations
//! and their JSON are listed in `README.md`. Symbol names use the
//! `limerick_mobile_*` / `LIMERICK_MOBILE_*` prefix.

#![deny(unsafe_op_in_unsafe_fn)]
#![allow(non_camel_case_types)]
// Safe C entry points validate their output pointers before writing through
// them. Marking them `unsafe` would make every Swift call an unsafe import
// without improving the checked contract.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod session;
pub mod wire;

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::ptr;
use std::slice;
use std::str;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use limerick_core::turn_inference::InferenceFailureKind;
use serde_json::{Map, Value, json};

use session::{OpError, OpenMode, OpenOptions, Session};

/// Largest request payload the boundary copies.
const MAX_REQUEST_BYTES: usize = 64 * 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct limerick_mobile_bytes_t {
    pub ptr: *const u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct limerick_mobile_owned_bytes_t {
    pub ptr: *mut u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum limerick_mobile_status_t {
    LIMERICK_MOBILE_OK = 0,
    LIMERICK_MOBILE_INVALID_ARGUMENT = 1,
    LIMERICK_MOBILE_INVALID_UTF8 = 2,
    LIMERICK_MOBILE_INVALID_HANDLE = 3,
    LIMERICK_MOBILE_TOO_LARGE = 4,
    LIMERICK_MOBILE_PROTOCOL_ERROR = 5,
    LIMERICK_MOBILE_CLOSED = 6,
    LIMERICK_MOBILE_INTERNAL_ERROR = 7,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum limerick_mobile_open_kind_t {
    LIMERICK_MOBILE_OPEN_NEW = 1,
    LIMERICK_MOBILE_OPEN_RESUME = 2,
}

pub type limerick_mobile_handle_t = u64;

/// Open sessions by handle. Each session is serialized by its own lock; the
/// Swift owner is an actor, so contention is only a safety net.
fn sessions() -> &'static Mutex<HashMap<limerick_mobile_handle_t, Arc<Mutex<Session>>>> {
    static SESSIONS: OnceLock<Mutex<HashMap<limerick_mobile_handle_t, Arc<Mutex<Session>>>>> =
        OnceLock::new();
    SESSIONS.get_or_init(Default::default)
}

fn next_handle() -> limerick_mobile_handle_t {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn panic_contained<F>(function: F) -> limerick_mobile_status_t
where
    F: FnOnce() -> limerick_mobile_status_t,
{
    catch_unwind(AssertUnwindSafe(function))
        .unwrap_or(limerick_mobile_status_t::LIMERICK_MOBILE_INTERNAL_ERROR)
}

fn empty_owned() -> limerick_mobile_owned_bytes_t {
    limerick_mobile_owned_bytes_t {
        ptr: ptr::null_mut(),
        len: 0,
    }
}

fn read_utf8(bytes: limerick_mobile_bytes_t) -> Result<String, limerick_mobile_status_t> {
    if bytes.len != 0 && bytes.ptr.is_null() {
        return Err(limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_ARGUMENT);
    }
    if bytes.len > MAX_REQUEST_BYTES {
        return Err(limerick_mobile_status_t::LIMERICK_MOBILE_TOO_LARGE);
    }
    if bytes.len == 0 {
        return Ok(String::new());
    }
    // SAFETY: null and length were checked above; the pointer is borrowed only
    // for this call and copied into an owned String before returning.
    let raw = unsafe { slice::from_raw_parts(bytes.ptr, bytes.len) };
    str::from_utf8(raw)
        .map(str::to_owned)
        .map_err(|_| limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_UTF8)
}

fn owned_bytes(bytes: Vec<u8>) -> limerick_mobile_owned_bytes_t {
    if bytes.is_empty() {
        return empty_owned();
    }
    let boxed = bytes.into_boxed_slice();
    let len = boxed.len();
    let ptr = Box::into_raw(boxed) as *mut u8;
    limerick_mobile_owned_bytes_t { ptr, len }
}

fn status_code(status: limerick_mobile_status_t) -> &'static str {
    match status {
        limerick_mobile_status_t::LIMERICK_MOBILE_OK => "ok",
        limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_ARGUMENT => "invalid_argument",
        limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_UTF8 => "invalid_utf8",
        limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_HANDLE => "invalid_handle",
        limerick_mobile_status_t::LIMERICK_MOBILE_TOO_LARGE => "too_large",
        limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR => "protocol_error",
        limerick_mobile_status_t::LIMERICK_MOBILE_CLOSED => "closed",
        limerick_mobile_status_t::LIMERICK_MOBILE_INTERNAL_ERROR => "internal_error",
    }
}

fn error_envelope(code: &str, message: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "ok": false,
        "error": { "code": code, "message": message },
    }))
    .unwrap_or_else(|_| b"{\"ok\":false,\"error\":{\"code\":\"internal_error\"}}".to_vec())
}

fn value_envelope(value: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({ "ok": true, "value": value }))
        .unwrap_or_else(|_| error_envelope("internal_error", "response is not serializable"))
}

/// Writes `bytes` to a caller-owned output pointer that the caller has
/// already checked for null.
fn publish(out_response: *mut limerick_mobile_owned_bytes_t, bytes: Vec<u8>) {
    // SAFETY: every caller checks `out_response` for null before calling.
    unsafe { ptr::write(out_response, owned_bytes(bytes)) };
}

fn fail(
    out_response: *mut limerick_mobile_owned_bytes_t,
    status: limerick_mobile_status_t,
    message: &str,
) -> limerick_mobile_status_t {
    publish(out_response, error_envelope(status_code(status), message));
    status
}

/// The status an operation error is reported with.
fn op_status(error: &OpError) -> limerick_mobile_status_t {
    match error.code {
        "protocol_error" => limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR,
        "internal_error" | "storage_error" | "content_unavailable" => {
            limerick_mobile_status_t::LIMERICK_MOBILE_INTERNAL_ERROR
        }
        // Rejections of a well-formed request (a request still open, a save
        // that cannot be opened): the envelope's code says which.
        _ => limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR,
    }
}

fn fail_op(
    out_response: *mut limerick_mobile_owned_bytes_t,
    error: &OpError,
) -> limerick_mobile_status_t {
    publish(out_response, error_envelope(error.code, &error.message));
    op_status(error)
}

/// Parses a request payload as a JSON object.
fn json_object(request: &str, what: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str::<Value>(request) {
        Ok(Value::Object(object)) => Ok(object),
        Ok(_) => Err(format!("{what} must be a JSON object")),
        Err(error) => Err(format!("{what} is not JSON: {error}")),
    }
}

fn string_field(object: &Map<String, Value>, field: &str) -> Result<String, OpError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| OpError::new("protocol_error", format!("`{field}` must be a string")))
}

fn optional_string(object: &Map<String, Value>, field: &str) -> Result<Option<String>, OpError> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(OpError::new(
            "protocol_error",
            format!("`{field}` must be a string"),
        )),
    }
}

/// A sequence, revision, or cursor: a number or `{"rawValue": n}`.
fn raw_number(object: &Map<String, Value>, field: &str) -> Result<Option<u64>, OpError> {
    let value = match object.get(field) {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Object(inner)) => inner.get("rawValue"),
        Some(other) => Some(other),
    };
    value
        .and_then(Value::as_u64)
        .map(Some)
        .ok_or_else(|| OpError::new("protocol_error", format!("`{field}` must be a number")))
}

fn required_number(object: &Map<String, Value>, field: &str) -> Result<u64, OpError> {
    raw_number(object, field)?
        .ok_or_else(|| OpError::new("protocol_error", format!("`{field}` is required")))
}

fn limit(object: &Map<String, Value>) -> Result<usize, OpError> {
    match raw_number(object, "limit")? {
        None => Ok(session::MAX_PAGE),
        Some(limit) if (1..=session::MAX_PAGE as u64).contains(&limit) => Ok(limit as usize),
        Some(_) => Err(OpError::new(
            "protocol_error",
            format!("`limit` must be between 1 and {}", session::MAX_PAGE),
        )),
    }
}

fn failure_kind(raw: &str) -> Result<InferenceFailureKind, OpError> {
    match raw {
        "transport" | "missing_terminal" => Ok(InferenceFailureKind::Transport),
        "protocol" => Ok(InferenceFailureKind::Protocol),
        "timed_out" => Ok(InferenceFailureKind::TimedOut),
        "interrupted" => Ok(InferenceFailureKind::Interrupted),
        other => Err(OpError::new(
            "protocol_error",
            format!("unknown failure kind `{other}`"),
        )),
    }
}

/// Runs one operation on a session.
fn run_operation(session: &mut Session, object: &Map<String, Value>) -> Result<Value, OpError> {
    let op = string_field(object, "op")?;
    match op.as_str() {
        "snapshot" => session.snapshot(),
        "submit" => session.submit(
            string_field(object, "text")?,
            optional_string(object, "draft_id")?,
            optional_string(object, "logical_request_id")?,
        ),
        "retry" => session.retry(&string_field(object, "logical_request_id")?),
        "answer_clarification" => session.answer_clarification(
            &string_field(object, "logical_request_id")?,
            &string_field(object, "choice_id")?,
        ),
        "stop" => session.stop(),
        "pending_endpoint" => Ok(session.pending_endpoint()),
        "resolve" => {
            let output = object
                .get("output")
                .ok_or_else(|| OpError::new("protocol_error", "`output` is required"))?;
            session.resolve(
                &string_field(object, "call_id")?,
                &string_field(object, "attempt_id")?,
                required_number(object, "base_revision")?,
                output,
            )
        }
        "fail" => session.fail(
            &string_field(object, "call_id")?,
            &string_field(object, "attempt_id")?,
            required_number(object, "base_revision")?,
            failure_kind(&string_field(object, "error_kind")?)?,
            optional_string(object, "message")?.unwrap_or_default(),
        ),
        "frame" => Ok(session.frame(
            &string_field(object, "call_id")?,
            &string_field(object, "attempt_id")?,
            required_number(object, "sequence")?,
            &string_field(object, "text")?,
        )),
        "read_events" => {
            session.events_after(raw_number(object, "after")?.unwrap_or(0), limit(object)?)
        }
        "read_event_page_before" => {
            session.events_before(required_number(object, "before")?, limit(object)?)
        }
        other => Err(OpError::new(
            "protocol_error",
            format!("unknown operation `{other}`"),
        )),
    }
}

/// Opens a session and returns its handle and opening snapshot.
///
/// The request is `{"save_path": ..., "mod_dir": ...}`. `OPEN_NEW` refuses a
/// save that already exists; `OPEN_RESUME` continues the save, or starts a
/// new game there when it does not exist.
#[unsafe(no_mangle)]
pub extern "C" fn limerick_mobile_open(
    kind: limerick_mobile_open_kind_t,
    request_json: limerick_mobile_bytes_t,
    out_handle: *mut limerick_mobile_handle_t,
    out_response: *mut limerick_mobile_owned_bytes_t,
) -> limerick_mobile_status_t {
    panic_contained(|| {
        if out_handle.is_null() || out_response.is_null() {
            return limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_ARGUMENT;
        }
        // Initialise both caller-owned outputs before any fallible work so a
        // caught panic leaves values the caller can safely inspect and free.
        // SAFETY: both pointers were checked for null above.
        unsafe {
            ptr::write(out_handle, 0);
            ptr::write(out_response, empty_owned());
        }
        let request = match read_utf8(request_json) {
            Ok(request) => request,
            Err(status) => return fail(out_response, status, "invalid open payload"),
        };
        let object = match json_object(&request, "open payload") {
            Ok(object) => object,
            Err(message) => {
                return fail(
                    out_response,
                    limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR,
                    &message,
                );
            }
        };
        let options = match (
            string_field(&object, "save_path"),
            string_field(&object, "mod_dir"),
        ) {
            (Ok(save_path), Ok(mod_dir)) => OpenOptions {
                save_path: PathBuf::from(save_path),
                mod_dir: PathBuf::from(mod_dir),
            },
            (Err(error), _) | (_, Err(error)) => return fail_op(out_response, &error),
        };
        let mode = match kind {
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW => OpenMode::New,
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_RESUME => OpenMode::Resume,
        };
        let session = match Session::open(mode, options) {
            Ok(session) => session,
            Err(error) => return fail_op(out_response, &error),
        };
        let snapshot = match session.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => return fail_op(out_response, &error),
        };
        let handle = next_handle();
        sessions()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(handle, Arc::new(Mutex::new(session)));
        // SAFETY: checked for null above.
        unsafe { ptr::write(out_handle, handle) };
        publish(out_response, value_envelope(snapshot));
        limerick_mobile_status_t::LIMERICK_MOBILE_OK
    })
}

/// Dispatches one JSON operation (`{"op": ...}`) on a session.
#[unsafe(no_mangle)]
pub extern "C" fn limerick_mobile_dispatch(
    handle: limerick_mobile_handle_t,
    operation_json: limerick_mobile_bytes_t,
    out_response: *mut limerick_mobile_owned_bytes_t,
) -> limerick_mobile_status_t {
    panic_contained(|| {
        if out_response.is_null() {
            return limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_ARGUMENT;
        }
        // SAFETY: the pointer was checked for null above.
        unsafe { ptr::write(out_response, empty_owned()) };
        let operation = match read_utf8(operation_json) {
            Ok(operation) => operation,
            Err(status) => return fail(out_response, status, "invalid operation payload"),
        };
        let object = match json_object(&operation, "operation") {
            Ok(object) => object,
            Err(message) => {
                return fail(
                    out_response,
                    limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR,
                    &message,
                );
            }
        };
        if !matches!(object.get("op"), Some(Value::String(_))) {
            return fail(
                out_response,
                limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR,
                "operation requires string field `op`",
            );
        }
        let session = sessions()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&handle)
            .cloned();
        let Some(session) = session else {
            return fail(
                out_response,
                limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_HANDLE,
                "invalid or closed session handle",
            );
        };
        let mut session = session.lock().unwrap_or_else(PoisonError::into_inner);
        match run_operation(&mut session, &object) {
            Ok(value) => {
                publish(out_response, value_envelope(value));
                limerick_mobile_status_t::LIMERICK_MOBILE_OK
            }
            Err(error) => fail_op(out_response, &error),
        }
    })
}

/// Closes a session: waits for any operation in progress, then releases
/// the session and its save lock.
#[unsafe(no_mangle)]
pub extern "C" fn limerick_mobile_close(
    handle: limerick_mobile_handle_t,
) -> limerick_mobile_status_t {
    panic_contained(|| {
        let session = sessions()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&handle);
        match session {
            Some(session) => {
                // Drain the session's lane before dropping it.
                drop(session.lock().unwrap_or_else(PoisonError::into_inner));
                drop(session);
                limerick_mobile_status_t::LIMERICK_MOBILE_OK
            }
            None => limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_HANDLE,
        }
    })
}

/// Releases one response returned by `limerick_mobile_open` or
/// `limerick_mobile_dispatch`.
#[unsafe(no_mangle)]
pub extern "C" fn limerick_mobile_owned_bytes_free(
    bytes: limerick_mobile_owned_bytes_t,
) -> limerick_mobile_status_t {
    panic_contained(|| {
        if bytes.ptr.is_null() {
            return if bytes.len == 0 {
                limerick_mobile_status_t::LIMERICK_MOBILE_OK
            } else {
                limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_ARGUMENT
            };
        }
        let slice = ptr::slice_from_raw_parts_mut(bytes.ptr, bytes.len);
        // SAFETY: a non-null pair is only ever produced by `owned_bytes` from
        // a boxed slice of exactly this length, and the caller frees it once.
        unsafe { drop(Box::from_raw(slice)) };
        limerick_mobile_status_t::LIMERICK_MOBILE_OK
    })
}

#[cfg(test)]
mod tests;
