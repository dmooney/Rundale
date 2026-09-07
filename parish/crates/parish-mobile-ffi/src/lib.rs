//! Owned, callback-free Swift/Rust boundary for the portable Parish runtime.
//!
//! The ABI deliberately carries only borrowed UTF-8 request bytes, owned JSON
//! response bytes, and an opaque session token. `Session` is a lifecycle
//! registry entry, not a second game store; the mobile engine owns the one
//! authoritative state machine behind `Backend`.

#![deny(unsafe_op_in_unsafe_fn)]
#![allow(non_camel_case_types)]
// Safe C entry points contain and validate their output pointers before the
// one internal write. Marking them `unsafe` would make every Swift call an
// unsafe import without improving the checked contract.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use serde_json::{Value, json};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::slice;
use std::str;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_EVENT_PAGE: usize = 100;
#[cfg(feature = "engine-api")]
const MAX_FAILURE_MESSAGE_BYTES: usize = 4 * 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct parish_mobile_bytes_t {
    pub ptr: *const u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct parish_mobile_owned_bytes_t {
    pub ptr: *mut u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum parish_mobile_status_t {
    PARISH_MOBILE_OK = 0,
    PARISH_MOBILE_INVALID_ARGUMENT = 1,
    PARISH_MOBILE_INVALID_UTF8 = 2,
    PARISH_MOBILE_INVALID_HANDLE = 3,
    PARISH_MOBILE_TOO_LARGE = 4,
    PARISH_MOBILE_PROTOCOL_ERROR = 5,
    PARISH_MOBILE_CLOSED = 6,
    PARISH_MOBILE_INTERNAL_ERROR = 7,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum parish_mobile_open_kind_t {
    PARISH_MOBILE_OPEN_NEW = 1,
    PARISH_MOBILE_OPEN_RESUME = 2,
}

pub type parish_mobile_handle_t = u64;

#[derive(Debug)]
struct BackendError {
    code: &'static str,
    message: String,
    status: parish_mobile_status_t,
}

impl BackendError {
    fn protocol(message: impl Into<String>) -> Self {
        Self {
            code: "protocol_error",
            message: message.into(),
            status: parish_mobile_status_t::PARISH_MOBILE_PROTOCOL_ERROR,
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal_error",
            message: message.into(),
            status: parish_mobile_status_t::PARISH_MOBILE_INTERNAL_ERROR,
        }
    }
}

/// The engine adapter is intentionally JSON-shaped so no engine object or
/// mutable pointer crosses this crate. The mobile-core owner can implement the
/// agreed operation DTOs without making Swift aware of its Rust layout.
trait Backend: Send {
    fn dispatch_json(&mut self, operation_json: &str) -> Result<String, BackendError>;
    fn close(&mut self) -> Result<(), BackendError> {
        Ok(())
    }
}

struct Session {
    backend: Mutex<Box<dyn Backend>>,
    closed: AtomicBool,
    poisoned: AtomicBool,
}

static SESSIONS: OnceLock<Mutex<HashMap<parish_mobile_handle_t, Arc<Session>>>> = OnceLock::new();
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

fn sessions() -> &'static Mutex<HashMap<parish_mobile_handle_t, Arc<Session>>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn panic_contained<F>(function: F) -> parish_mobile_status_t
where
    F: FnOnce() -> parish_mobile_status_t,
{
    match catch_unwind(AssertUnwindSafe(function)) {
        Ok(status) => status,
        Err(_) => parish_mobile_status_t::PARISH_MOBILE_INTERNAL_ERROR,
    }
}

fn clear_owned_output(out_response: *mut parish_mobile_owned_bytes_t) {
    if !out_response.is_null() {
        // SAFETY: callers validate this output pointer before entering the
        // panic-contained boundary. Zeroing it first makes a caught panic
        // safe for the foreign caller to release.
        unsafe {
            ptr::write(
                out_response,
                parish_mobile_owned_bytes_t {
                    ptr: ptr::null_mut(),
                    len: 0,
                },
            );
        }
    }
}

fn read_utf8(bytes: parish_mobile_bytes_t, limit: usize) -> Result<String, parish_mobile_status_t> {
    if bytes.len != 0 && bytes.ptr.is_null() {
        return Err(parish_mobile_status_t::PARISH_MOBILE_INVALID_ARGUMENT);
    }
    if bytes.len > limit {
        return Err(parish_mobile_status_t::PARISH_MOBILE_TOO_LARGE);
    }
    if bytes.len == 0 {
        return Ok(String::new());
    }
    // SAFETY: null and length were checked above; the pointer is borrowed only
    // for this call and copied into an owned String before returning.
    let raw = unsafe { slice::from_raw_parts(bytes.ptr, bytes.len) };
    str::from_utf8(raw)
        .map(str::to_owned)
        .map_err(|_| parish_mobile_status_t::PARISH_MOBILE_INVALID_UTF8)
}

fn owned_bytes(bytes: Vec<u8>) -> Result<parish_mobile_owned_bytes_t, BackendError> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(BackendError {
            code: "response_too_large",
            message: format!("response exceeds {} bytes", MAX_RESPONSE_BYTES),
            status: parish_mobile_status_t::PARISH_MOBILE_TOO_LARGE,
        });
    }
    if bytes.is_empty() {
        return Ok(parish_mobile_owned_bytes_t {
            ptr: ptr::null_mut(),
            len: 0,
        });
    }
    let boxed = bytes.into_boxed_slice();
    let len = boxed.len();
    let ptr = Box::into_raw(boxed) as *mut u8;
    Ok(parish_mobile_owned_bytes_t { ptr, len })
}

fn release_owned_bytes(bytes: parish_mobile_owned_bytes_t) {
    if bytes.ptr.is_null() {
        return;
    }
    // SAFETY: this pair was produced by `owned_bytes` and has not yet been
    // published to the foreign caller.
    let slice = ptr::slice_from_raw_parts_mut(bytes.ptr, bytes.len);
    unsafe { drop(Box::from_raw(slice)) };
}

fn response_envelope(value: String) -> Result<Vec<u8>, BackendError> {
    let parsed: Value = serde_json::from_str(&value).map_err(|error| {
        BackendError::protocol(format!("engine returned invalid JSON: {error}"))
    })?;
    serde_json::to_vec(&json!({ "ok": true, "value": parsed }))
        .map_err(|error| BackendError::internal(format!("encode response envelope: {error}")))
}

fn error_envelope(error: &BackendError) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "ok": false,
        "error": {
            "code": error.code,
            "message": error.message,
        }
    }))
    .unwrap_or_else(|_| b"{\"ok\":false,\"error\":{\"code\":\"internal_error\"}}".to_vec())
}

fn write_owned(
    out_response: *mut parish_mobile_owned_bytes_t,
    bytes: Vec<u8>,
) -> parish_mobile_status_t {
    if out_response.is_null() {
        return parish_mobile_status_t::PARISH_MOBILE_INVALID_ARGUMENT;
    }
    match owned_bytes(bytes) {
        Ok(result) => {
            // SAFETY: the output pointer was checked for null and points to
            // caller-owned storage for this call.
            unsafe { ptr::write(out_response, result) };
            parish_mobile_status_t::PARISH_MOBILE_OK
        }
        Err(error) => {
            let result = owned_bytes(error_envelope(&error));
            if let Ok(result) = result {
                // SAFETY: the output pointer was checked for null above.
                unsafe { ptr::write(out_response, result) };
            }
            error.status
        }
    }
}

fn write_error(
    out_response: *mut parish_mobile_owned_bytes_t,
    status: parish_mobile_status_t,
    message: impl Into<String>,
) -> parish_mobile_status_t {
    let error = BackendError {
        code: match status {
            parish_mobile_status_t::PARISH_MOBILE_INVALID_ARGUMENT => "invalid_argument",
            parish_mobile_status_t::PARISH_MOBILE_INVALID_UTF8 => "invalid_utf8",
            parish_mobile_status_t::PARISH_MOBILE_INVALID_HANDLE => "invalid_handle",
            parish_mobile_status_t::PARISH_MOBILE_TOO_LARGE => "too_large",
            parish_mobile_status_t::PARISH_MOBILE_CLOSED => "closed",
            parish_mobile_status_t::PARISH_MOBILE_INTERNAL_ERROR => "internal_error",
            _ => "protocol_error",
        },
        message: message.into(),
        status,
    };
    if out_response.is_null() {
        return parish_mobile_status_t::PARISH_MOBILE_INVALID_ARGUMENT;
    }
    let bytes = error_envelope(&error);
    match owned_bytes(bytes) {
        Ok(result) => {
            // SAFETY: the output pointer was checked for null above.
            unsafe { ptr::write(out_response, result) };
        }
        Err(_) => return parish_mobile_status_t::PARISH_MOBILE_INTERNAL_ERROR,
    }
    status
}

fn lookup(handle: parish_mobile_handle_t) -> Result<Arc<Session>, parish_mobile_status_t> {
    sessions()
        .lock()
        .map_err(|_| parish_mobile_status_t::PARISH_MOBILE_INTERNAL_ERROR)?
        .get(&handle)
        .cloned()
        .ok_or(parish_mobile_status_t::PARISH_MOBILE_INVALID_HANDLE)
}

fn validate_operation(operation: &str) -> Result<(), BackendError> {
    let value: Value = serde_json::from_str(operation)
        .map_err(|error| BackendError::protocol(format!("operation is not JSON: {error}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| BackendError::protocol("operation must be a JSON object"))?;
    let operation = object
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| BackendError::protocol("operation requires string field `op`"))?;
    let allowed = [
        "submit",
        "retry",
        "stop",
        "fail",
        "receive_failure",
        "receive_frame",
        "receive_candidate",
        "read_events",
        "read_event_page",
        "snapshot",
        "pending_endpoint",
    ];
    if !allowed.contains(&operation) {
        return Err(BackendError::protocol(format!(
            "unknown mobile operation `{operation}`"
        )));
    }
    if matches!(operation, "read_events" | "read_event_page")
        && object
            .get("limit")
            .and_then(Value::as_u64)
            .is_some_and(|limit| limit == 0 || limit > MAX_EVENT_PAGE as u64)
    {
        return Err(BackendError::protocol(format!(
            "read_events limit must be between 1 and {MAX_EVENT_PAGE}"
        )));
    }
    Ok(())
}

fn open_backend(
    kind: parish_mobile_open_kind_t,
    request: &str,
) -> Result<(Box<dyn Backend>, String), BackendError> {
    #[cfg(feature = "engine-api")]
    {
        core_backend::open(kind, request)
    }
    #[cfg(not(feature = "engine-api"))]
    {
        let _ = (kind, request);
        Err(BackendError::internal(
            "portable parish-core mobile API is not enabled for this build",
        ))
    }
}

#[cfg(feature = "engine-api")]
mod core_backend {
    use super::{Backend, BackendError, MAX_EVENT_PAGE, Value, parish_mobile_open_kind_t};
    use parish_core::mobile::{
        DraftId, EndpointCandidate, EndpointFailureKind, EndpointFrame, EventCursor,
        ExecutionAttemptId, LogicalRequestId, MobileSave, MobileSession, StateRevision,
        StreamUpdate,
    };
    use serde::{Serialize, de::DeserializeOwned};
    use serde_json::Map;
    use std::path::Path;

    fn parse<T: DeserializeOwned>(value: &Value, field: &str) -> Result<T, BackendError> {
        serde_json::from_value(value.clone())
            .map_err(|error| BackendError::protocol(format!("invalid `{field}`: {error}")))
    }

    fn required<'a>(
        object: &'a Map<String, Value>,
        field: &str,
    ) -> Result<&'a Value, BackendError> {
        object
            .get(field)
            .ok_or_else(|| BackendError::protocol(format!("operation requires `{field}`")))
    }

    fn value_with_alias<'a>(
        object: &'a Map<String, Value>,
        field: &str,
        alias: &str,
    ) -> Option<&'a Value> {
        object.get(field).or_else(|| object.get(alias))
    }

    fn optional_id<T>(
        object: &Map<String, Value>,
        field: &str,
        alias: &str,
    ) -> Result<Option<T>, BackendError>
    where
        T: DeserializeOwned,
    {
        value_with_alias(object, field, alias)
            .map(|value| parse(value, field))
            .transpose()
    }

    fn parse_id<T>(object: &Map<String, Value>, field: &str, alias: &str) -> Result<T, BackendError>
    where
        T: DeserializeOwned,
    {
        let value = value_with_alias(object, field, alias)
            .ok_or_else(|| BackendError::protocol(format!("operation requires `{field}`")))?;
        parse(value, field)
    }

    fn parse_raw_value<T>(value: &Value, field: &str) -> Result<T, BackendError>
    where
        T: DeserializeOwned,
    {
        // The canonical wire form is `{ "rawValue": ... }`, matching the
        // Swift RawRepresentable DTOs. Numeric cursors/revisions are accepted
        // as a compatibility convenience for older clients.
        parse(value, field)
    }

    fn parse_cursor(value: &Value, field: &str) -> Result<EventCursor, BackendError> {
        if let Some(raw) = value.as_u64() {
            return Ok(EventCursor::new(raw));
        }
        parse_raw_value(value, field)
    }

    fn parse_revision(value: &Value, field: &str) -> Result<StateRevision, BackendError> {
        if let Some(raw) = value.as_u64() {
            return Ok(StateRevision::new(raw));
        }
        parse_raw_value(value, field)
    }

    fn parse_stream_update(value: Option<&Value>) -> Result<StreamUpdate, BackendError> {
        let Some(value) = value else {
            return Ok(StreamUpdate::Replace);
        };
        parse(value, "stream_update")
    }

    fn failure_request(
        object: &Map<String, Value>,
    ) -> Result<
        (
            ExecutionAttemptId,
            StateRevision,
            EndpointFailureKind,
            String,
        ),
        BackendError,
    > {
        let attempt_id = parse_id::<ExecutionAttemptId>(object, "attempt_id", "attemptID")?;
        let base_revision_value = value_with_alias(object, "base_revision", "baseRevision")
            .ok_or_else(|| BackendError::protocol("operation requires `base_revision`"))?;
        let base_revision = parse_revision(base_revision_value, "base_revision")?;
        let kind_value = value_with_alias(object, "error_kind", "errorKind")
            .or_else(|| object.get("kind"))
            .ok_or_else(|| BackendError::protocol("operation requires `error_kind`"))?;
        let kind: String = parse(kind_value, "error_kind")?;
        let failure_kind = match kind.as_str() {
            "transport" => EndpointFailureKind::Transport,
            "protocol" => EndpointFailureKind::Protocol,
            "missing_terminal" => EndpointFailureKind::MissingTerminal,
            "interrupted" => EndpointFailureKind::Interrupted,
            _ => {
                return Err(BackendError::protocol(
                    "failure kind must be `transport`, `protocol`, `missing_terminal`, or `interrupted`",
                ));
            }
        };
        let message: String = parse(required(object, "message")?, "message")?;
        if message.len() > super::MAX_FAILURE_MESSAGE_BYTES || message.chars().any(char::is_control)
        {
            return Err(BackendError::protocol(
                "failure message must be bounded and free of control characters",
            ));
        }
        Ok((attempt_id, base_revision, failure_kind, message))
    }

    fn as_json<T: Serialize>(value: T) -> Result<Value, BackendError> {
        serde_json::to_value(value)
            .map_err(|error| BackendError::internal(format!("encode operation result: {error}")))
    }

    struct CoreBackend {
        session: MobileSession,
    }

    impl Backend for CoreBackend {
        fn dispatch_json(&mut self, operation_json: &str) -> Result<String, BackendError> {
            let value: Value = serde_json::from_str(operation_json).map_err(|error| {
                BackendError::protocol(format!("operation is not JSON: {error}"))
            })?;
            let object = value
                .as_object()
                .ok_or_else(|| BackendError::protocol("operation must be a JSON object"))?;
            let operation = object
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| BackendError::protocol("operation requires string field `op`"))?;

            let result = match operation {
                "submit" => {
                    let text: String = parse(required(object, "text")?, "text")?;
                    let logical_request_id = optional_id::<LogicalRequestId>(
                        object,
                        "logical_request_id",
                        "logicalRequestID",
                    )?;
                    let draft_id = optional_id::<DraftId>(object, "draft_id", "draftID")?;
                    as_json(
                        self.session
                            .submit(logical_request_id, text, draft_id)
                            .map_err(|error| BackendError::protocol(error.to_string()))?,
                    )?
                }
                "retry" => {
                    let logical_request_id = parse_id::<LogicalRequestId>(
                        object,
                        "logical_request_id",
                        "logicalRequestID",
                    )?;
                    as_json(
                        self.session
                            .retry(&logical_request_id)
                            .map_err(|error| BackendError::protocol(error.to_string()))?,
                    )?
                }
                "stop" => {
                    let attempt_id = value_with_alias(object, "attempt_id", "attemptID")
                        .map(|value| parse(value, "attempt_id"))
                        .transpose()?
                        .or_else(|| {
                            let snapshot = self.session.snapshot();
                            let request_id = snapshot.active_request_id?;
                            snapshot
                                .requests
                                .iter()
                                .find(|request| request.id == request_id)
                                .and_then(|request| request.current_attempt_id.clone())
                        })
                        .unwrap_or_else(|| ExecutionAttemptId::new(""));
                    as_json(
                        self.session
                            .stop(&attempt_id)
                            .map_err(|error| BackendError::protocol(error.to_string()))?,
                    )?
                }
                "fail" => {
                    let attempt_id =
                        parse_id::<ExecutionAttemptId>(object, "attempt_id", "attemptID")?;
                    let message: String = parse(required(object, "message")?, "message")?;
                    as_json(
                        self.session
                            .fail(&attempt_id, message)
                            .map_err(|error| BackendError::protocol(error.to_string()))?,
                    )?
                }
                "receive_failure" => {
                    let (attempt_id, base_revision, failure_kind, message) =
                        failure_request(object)?;
                    as_json(
                        self.session
                            .receive_failure(&attempt_id, base_revision, failure_kind, message)
                            .map_err(|error| BackendError::protocol(error.to_string()))?,
                    )?
                }
                "receive_frame" => {
                    let attempt_id =
                        parse_id::<ExecutionAttemptId>(object, "attempt_id", "attemptID")?;
                    let base_revision_value =
                        value_with_alias(object, "base_revision", "baseRevision").ok_or_else(
                            || BackendError::protocol("operation requires `base_revision`"),
                        )?;
                    let base_revision = parse_revision(base_revision_value, "base_revision")?;
                    let sequence: u64 = parse(required(object, "sequence")?, "sequence")?;
                    let text: String = parse(required(object, "text")?, "text")?;
                    let stream_update = parse_stream_update(value_with_alias(
                        object,
                        "stream_update",
                        "streamUpdate",
                    ))?;
                    let done = object.get("done").and_then(Value::as_bool).unwrap_or(false);
                    as_json(
                        self.session
                            .receive_frame(EndpointFrame {
                                attempt_id,
                                base_revision,
                                sequence,
                                text,
                                stream_update,
                                done,
                            })
                            .map_err(|error| BackendError::protocol(error.to_string()))?,
                    )?
                }
                "receive_candidate" => {
                    let attempt_id =
                        parse_id::<ExecutionAttemptId>(object, "attempt_id", "attemptID")?;
                    let base_revision_value =
                        value_with_alias(object, "base_revision", "baseRevision").ok_or_else(
                            || BackendError::protocol("operation requires `base_revision`"),
                        )?;
                    let base_revision = parse_revision(base_revision_value, "base_revision")?;
                    let dialogue: String = parse(required(object, "dialogue")?, "dialogue")?;
                    let metadata = object
                        .get("metadata")
                        .map(|value| parse(value, "metadata"))
                        .transpose()?
                        .unwrap_or_default();
                    let structured = object
                        .get("structured")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    as_json(
                        self.session
                            .receive_candidate(EndpointCandidate {
                                attempt_id,
                                base_revision,
                                dialogue,
                                metadata,
                                structured,
                            })
                            .map_err(|error| BackendError::protocol(error.to_string()))?,
                    )?
                }
                "read_events" => {
                    let after = object
                        .get("after")
                        .or_else(|| object.get("cursor"))
                        .map(|value| parse_cursor(value, "after"))
                        .transpose()?;
                    let limit = object
                        .get("limit")
                        .map(|value| parse(value, "limit"))
                        .transpose()?
                        .unwrap_or(MAX_EVENT_PAGE);
                    as_json(self.session.read_events(after, limit))?
                }
                "read_event_page" => {
                    let after = object
                        .get("after")
                        .or_else(|| object.get("cursor"))
                        .map(|value| parse_cursor(value, "after"))
                        .transpose()?;
                    let limit = object
                        .get("limit")
                        .map(|value| parse(value, "limit"))
                        .transpose()?
                        .unwrap_or(MAX_EVENT_PAGE);
                    as_json(
                        self.session
                            .read_event_page(after, limit)
                            .map_err(|error| BackendError::internal(error.to_string()))?,
                    )?
                }
                "snapshot" => as_json(self.session.snapshot())?,
                "pending_endpoint" => as_json(self.session.take_pending_invocation())?,
                other => {
                    return Err(BackendError::protocol(format!(
                        "unknown mobile operation `{other}`"
                    )));
                }
            };

            serde_json::to_string(&result).map_err(|error| {
                BackendError::internal(format!("encode operation result: {error}"))
            })
        }
    }

    pub(super) fn open(
        kind: parish_mobile_open_kind_t,
        request: &str,
    ) -> Result<(Box<dyn Backend>, String), BackendError> {
        let options: Value = serde_json::from_str(request).map_err(|error| {
            BackendError::protocol(format!("open payload is not JSON: {error}"))
        })?;
        let save_path = options
            .as_object()
            .and_then(|object| object.get("save_path"))
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty());
        let session = match kind {
            parish_mobile_open_kind_t::PARISH_MOBILE_OPEN_NEW => if let Some(path) = save_path {
                MobileSession::open_new_sqlite(Path::new(path))
            } else {
                MobileSession::open_new()
            }
            .map_err(|error| BackendError::internal(error.to_string()))?,
            parish_mobile_open_kind_t::PARISH_MOBILE_OPEN_RESUME => {
                if let Some(path) = save_path {
                    let path = Path::new(path);
                    let existed_before = path
                        .try_exists()
                        .map_err(|error| BackendError::internal(error.to_string()))?;
                    match MobileSession::open_resume_sqlite(path)
                        .map_err(|error| BackendError::internal(error.to_string()))?
                    {
                        Some(session) => session,
                        None if !existed_before => MobileSession::open_new_sqlite(path)
                            .map_err(|error| BackendError::internal(error.to_string()))?,
                        None => {
                            return Err(BackendError::internal(
                                "existing mobile save contains no resumable game",
                            ));
                        }
                    }
                } else {
                    let save: MobileSave = serde_json::from_str(request).map_err(|error| {
                        BackendError::protocol(format!(
                            "resume payload is not a MobileSave: {error}"
                        ))
                    })?;
                    MobileSession::open_resume(save)
                        .map_err(|error| BackendError::internal(error.to_string()))?
                }
            }
        };
        let mut backend = CoreBackend { session };
        let opening = backend.dispatch_json(r#"{"op":"snapshot"}"#)?;
        Ok((Box::new(backend), opening))
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn parish_mobile_open(
    kind: parish_mobile_open_kind_t,
    request_json: parish_mobile_bytes_t,
    out_handle: *mut parish_mobile_handle_t,
    out_response: *mut parish_mobile_owned_bytes_t,
) -> parish_mobile_status_t {
    panic_contained(|| {
        if out_handle.is_null() || out_response.is_null() {
            return parish_mobile_status_t::PARISH_MOBILE_INVALID_ARGUMENT;
        }
        // Initialize both foreign-owned outputs before any fallible work. If
        // a panic is caught below, the caller can safely inspect/release the
        // zero values.
        clear_owned_output(out_response);
        // SAFETY: the output pointer was checked for null above.
        unsafe { ptr::write(out_handle, 0) };
        let request = match read_utf8(request_json, MAX_REQUEST_BYTES) {
            Ok(request) => request,
            Err(status) => return write_error(out_response, status, "invalid open payload"),
        };
        if let Err(error) = validate_open_request(kind, &request) {
            return write_error(out_response, error.status, error.message);
        }
        let (backend, opening) = match open_backend(kind, &request) {
            Ok(result) => result,
            Err(error) => return write_error(out_response, error.status, error.message),
        };
        // Finish all fallible response serialization/allocation before
        // publishing the session token. A failure here drops the backend and
        // its storage lock without leaving a registry entry to roll back.
        let opening = match response_envelope(opening) {
            Ok(bytes) => bytes,
            Err(error) => return write_error(out_response, error.status, error.message),
        };
        let opening = match owned_bytes(opening) {
            Ok(bytes) => bytes,
            Err(error) => return write_error(out_response, error.status, error.message),
        };
        let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
        let session = Arc::new(Session {
            backend: Mutex::new(backend),
            closed: AtomicBool::new(false),
            poisoned: AtomicBool::new(false),
        });
        if let Ok(mut registry) = sessions().lock() {
            registry.insert(handle, session);
        } else {
            release_owned_bytes(opening);
            return write_error(
                out_response,
                parish_mobile_status_t::PARISH_MOBILE_INTERNAL_ERROR,
                "session registry lock poisoned",
            );
        }
        // SAFETY: both output pointers were checked for null above.
        unsafe { ptr::write(out_handle, handle) };
        // `opening` was allocated successfully before registry insertion, so
        // this write cannot fail and cannot leak a published handle.
        unsafe { ptr::write(out_response, opening) };
        parish_mobile_status_t::PARISH_MOBILE_OK
    })
}

fn validate_open_request(
    kind: parish_mobile_open_kind_t,
    request: &str,
) -> Result<(), BackendError> {
    let value: Value = serde_json::from_str(request)
        .map_err(|error| BackendError::protocol(format!("open payload is not JSON: {error}")))?;
    if !value.is_object() {
        return Err(BackendError::protocol("open payload must be a JSON object"));
    }
    if matches!(kind, parish_mobile_open_kind_t::PARISH_MOBILE_OPEN_RESUME)
        && value.as_object().is_some_and(|object| object.is_empty())
    {
        return Err(BackendError::protocol("resume payload cannot be empty"));
    }
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "C" fn parish_mobile_dispatch(
    handle: parish_mobile_handle_t,
    operation_json: parish_mobile_bytes_t,
    out_response: *mut parish_mobile_owned_bytes_t,
) -> parish_mobile_status_t {
    panic_contained(|| {
        if out_response.is_null() {
            return parish_mobile_status_t::PARISH_MOBILE_INVALID_ARGUMENT;
        }
        clear_owned_output(out_response);
        let operation = match read_utf8(operation_json, MAX_REQUEST_BYTES) {
            Ok(operation) => operation,
            Err(status) => return write_error(out_response, status, "invalid operation payload"),
        };
        if let Err(error) = validate_operation(&operation) {
            return write_error(out_response, error.status, error.message);
        }
        let session = match lookup(handle) {
            Ok(session) => session,
            Err(status) => return write_error(out_response, status, "invalid or disposed handle"),
        };
        if session.closed.load(Ordering::Acquire) || session.poisoned.load(Ordering::Acquire) {
            return write_error(
                out_response,
                parish_mobile_status_t::PARISH_MOBILE_CLOSED,
                "session is closed",
            );
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut backend = session
                .backend
                .lock()
                .map_err(|_| BackendError::internal("session mutation lane poisoned"))?;
            // The check above is only a fast path. Close removes the registry
            // entry and flips this flag before waiting for this same mutex;
            // recheck under the mutation lane so a queued dispatch cannot
            // start after close has taken ownership of shutdown.
            if session.closed.load(Ordering::Acquire) || session.poisoned.load(Ordering::Acquire) {
                return Err(BackendError {
                    code: "closed",
                    message: "session is closed".to_owned(),
                    status: parish_mobile_status_t::PARISH_MOBILE_CLOSED,
                });
            }
            backend.dispatch_json(&operation)
        }));
        let value = match result {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => return write_error(out_response, error.status, error.message),
            Err(_) => {
                session.poisoned.store(true, Ordering::Release);
                return write_error(
                    out_response,
                    parish_mobile_status_t::PARISH_MOBILE_INTERNAL_ERROR,
                    "session panicked and is closed for recovery",
                );
            }
        };
        match response_envelope(value) {
            Ok(bytes) => write_owned(out_response, bytes),
            Err(error) => write_error(out_response, error.status, error.message),
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn parish_mobile_close(handle: parish_mobile_handle_t) -> parish_mobile_status_t {
    panic_contained(|| {
        let session = match sessions().lock() {
            Ok(mut registry) => registry.remove(&handle),
            Err(_) => {
                return parish_mobile_status_t::PARISH_MOBILE_INTERNAL_ERROR;
            }
        };
        let Some(session) = session else {
            return parish_mobile_status_t::PARISH_MOBILE_INVALID_HANDLE;
        };
        session.closed.store(true, Ordering::Release);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut backend = session
                .backend
                .lock()
                .map_err(|_| BackendError::internal("session mutation lane poisoned"))?;
            backend.close()
        }));
        match result {
            Ok(Ok(())) => parish_mobile_status_t::PARISH_MOBILE_OK,
            Ok(Err(error)) => error.status,
            Err(_) => parish_mobile_status_t::PARISH_MOBILE_INTERNAL_ERROR,
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn parish_mobile_owned_bytes_free(
    bytes: parish_mobile_owned_bytes_t,
) -> parish_mobile_status_t {
    panic_contained(|| {
        if bytes.len != 0 && bytes.ptr.is_null() {
            return parish_mobile_status_t::PARISH_MOBILE_INVALID_ARGUMENT;
        }
        if bytes.ptr.is_null() {
            return parish_mobile_status_t::PARISH_MOBILE_OK;
        }
        release_owned_bytes(bytes);
        parish_mobile_status_t::PARISH_MOBILE_OK
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_validation_keeps_protocol_bounded() {
        assert!(validate_operation(r#"{"op":"snapshot"}"#).is_ok());
        assert!(validate_operation(r#"{"op":"unknown"}"#).is_err());
        assert!(validate_operation(r#"{"op":"read_events","limit":101}"#).is_err());
    }

    #[test]
    fn owned_buffer_round_trip_is_utf8() {
        let value = owned_bytes("céad 🌧️".as_bytes().to_vec()).unwrap();
        let text = unsafe {
            str::from_utf8(slice::from_raw_parts(value.ptr, value.len))
                .unwrap()
                .to_owned()
        };
        assert_eq!(text, "céad 🌧️");
        assert_eq!(
            parish_mobile_owned_bytes_free(value),
            parish_mobile_status_t::PARISH_MOBILE_OK
        );
    }

    #[test]
    fn error_envelope_is_structured() {
        let error = BackendError::protocol("bad operation");
        let decoded: Value = serde_json::from_slice(&error_envelope(&error)).unwrap();
        assert_eq!(decoded["ok"], false);
        assert_eq!(decoded["error"]["code"], "protocol_error");
    }

    #[test]
    fn panic_safe_output_starts_empty() {
        let mut output = parish_mobile_owned_bytes_t {
            ptr: ptr::dangling_mut::<u8>(),
            len: 99,
        };
        clear_owned_output(&mut output);
        assert!(output.ptr.is_null());
        assert_eq!(output.len, 0);
    }

    #[cfg(feature = "engine-api")]
    #[test]
    fn engine_round_trip_uses_owned_json_and_disposes_handle() {
        fn borrowed(value: &str) -> parish_mobile_bytes_t {
            parish_mobile_bytes_t {
                ptr: value.as_ptr(),
                len: value.len(),
            }
        }

        fn take(response: parish_mobile_owned_bytes_t) -> Value {
            let value = unsafe {
                serde_json::from_slice(slice::from_raw_parts(response.ptr, response.len)).unwrap()
            };
            assert_eq!(
                parish_mobile_owned_bytes_free(response),
                parish_mobile_status_t::PARISH_MOBILE_OK
            );
            value
        }

        let mut handle = 0;
        let mut opening = parish_mobile_owned_bytes_t {
            ptr: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            parish_mobile_open(
                parish_mobile_open_kind_t::PARISH_MOBILE_OPEN_NEW,
                borrowed("{}"),
                &mut handle,
                &mut opening,
            ),
            parish_mobile_status_t::PARISH_MOBILE_OK
        );
        let opening = take(opening);
        assert_eq!(opening["ok"], true);
        assert!(opening["value"]["sessionID"].is_string());

        let mut submitted = parish_mobile_owned_bytes_t {
            ptr: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            parish_mobile_dispatch(
                handle,
                borrowed(r#"{"op":"submit","text":"/look"}"#),
                &mut submitted,
            ),
            parish_mobile_status_t::PARISH_MOBILE_OK
        );
        let submitted = take(submitted);
        assert_eq!(submitted["ok"], true);
        assert_eq!(submitted["value"]["accepted"], true);
        assert_eq!(submitted["value"]["events"].as_array().unwrap().len(), 3);

        let mut pending = parish_mobile_owned_bytes_t {
            ptr: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            parish_mobile_dispatch(
                handle,
                borrowed(r#"{"op":"submit","text":"ask Peig about the wall"}"#),
                &mut pending,
            ),
            parish_mobile_status_t::PARISH_MOBILE_OK
        );
        let pending = take(pending);
        let invocation = &pending["value"]["endpointInvocation"];
        let attempt_id = invocation["attemptID"].clone();
        let base_revision = invocation["baseRevision"].clone();
        assert!(invocation.is_object());

        let failure = format!(
            "{{\"op\":\"receive_failure\",\"attemptID\":{},\"baseRevision\":{},\"errorKind\":\"transport\",\"message\":\"Endpoint unavailable\"}}",
            serde_json::to_string(&attempt_id).unwrap(),
            serde_json::to_string(&base_revision).unwrap(),
        );
        let mut failed = parish_mobile_owned_bytes_t {
            ptr: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            parish_mobile_dispatch(handle, borrowed(&failure), &mut failed),
            parish_mobile_status_t::PARISH_MOBILE_OK
        );
        let failed = take(failed);
        assert_eq!(failed["value"]["terminalOutcome"], "failed");

        assert_eq!(
            parish_mobile_close(handle),
            parish_mobile_status_t::PARISH_MOBILE_OK
        );
        let mut after_close = parish_mobile_owned_bytes_t {
            ptr: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            parish_mobile_dispatch(handle, borrowed(r#"{"op":"snapshot"}"#), &mut after_close),
            parish_mobile_status_t::PARISH_MOBILE_INVALID_HANDLE
        );
        let after_close = take(after_close);
        assert_eq!(after_close["ok"], false);
    }

    #[cfg(feature = "engine-api")]
    #[test]
    fn resume_does_not_replace_an_existing_empty_save() {
        fn borrowed(value: &str) -> parish_mobile_bytes_t {
            parish_mobile_bytes_t {
                ptr: value.as_ptr(),
                len: value.len(),
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("empty.sqlite");
        // The core's resume API bootstraps a missing path to an initialized
        // but game-less database. A second resume must reject that existing
        // empty save rather than silently replacing it with a new game.
        assert!(
            parish_core::mobile::MobileSession::open_resume_sqlite(&path)
                .unwrap()
                .is_none()
        );
        assert!(path.exists());
        let request = format!(r#"{{"save_path":"{}"}}"#, path.display());
        let mut handle = 0;
        let mut response = parish_mobile_owned_bytes_t {
            ptr: ptr::null_mut(),
            len: 0,
        };
        let status = parish_mobile_open(
            parish_mobile_open_kind_t::PARISH_MOBILE_OPEN_RESUME,
            borrowed(&request),
            &mut handle,
            &mut response,
        );
        assert_eq!(status, parish_mobile_status_t::PARISH_MOBILE_INTERNAL_ERROR);
        assert_eq!(handle, 0);
        assert!(!response.ptr.is_null());
        release_owned_bytes(response);
    }
}
