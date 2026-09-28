//! Callback-free C boundary between the native iPhone client and the shared
//! Limerick engine.
//!
//! The ABI carries only borrowed UTF-8 request bytes, owned JSON response
//! bytes, and an opaque session handle (see `include/limerick_mobile_ffi.h`).
//! Every response is a JSON envelope: `{"ok": true, "value": ...}` or
//! `{"ok": false, "error": {"code": ..., "message": ...}}`.
//!
//! This crate keeps the boundary shape from the `ios-port` branch and links
//! the shared engine (`limerick-core` with the `mobile` feature). It does not
//! run gameplay yet: `ios-port`'s mobile-only runtime was not carried over
//! (ADR-025), and wiring these entry points to the shared `TurnEngine` is
//! #2044. Until then every session request answers with the structured
//! `not_wired` error below, so the app can show an honest state instead of
//! failing opaquely. Symbol names use the `limerick_mobile_*` / `LIMERICK_MOBILE_*` prefix.

#![deny(unsafe_op_in_unsafe_fn)]
#![allow(non_camel_case_types)]
// Safe C entry points validate their output pointers before writing through
// them. Marking them `unsafe` would make every Swift call an unsafe import
// without improving the checked contract.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use limerick_core::persistence::SAVE_FORMAT_VERSION;
use serde_json::{Value, json};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::slice;
use std::str;

/// Largest request payload the boundary copies.
const MAX_REQUEST_BYTES: usize = 64 * 1024;

/// Error code for requests the shared engine does not serve through this
/// boundary yet. Swift maps it to a dedicated "not yet wired" state.
pub const NOT_WIRED_CODE: &str = "not_wired";

/// Issue that wires the boundary to the shared turn API.
pub const NOT_WIRED_ISSUE: u32 = 2044;

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

/// The envelope returned for every session request until #2044 lands. The
/// `engine` object is read from the linked shared engine, so a caller can see
/// which save format the engine it is linked against writes.
fn not_wired_envelope(operation: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "ok": false,
        "error": {
            "code": NOT_WIRED_CODE,
            "message": format!(
                "`{operation}` is not yet wired to the shared Limerick engine (#{NOT_WIRED_ISSUE})."
            ),
            "issue": NOT_WIRED_ISSUE,
            "engine": { "save_format_version": SAVE_FORMAT_VERSION },
        }
    }))
    .unwrap_or_else(|_| error_envelope(NOT_WIRED_CODE, "not yet wired"))
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

/// Parses a request payload as a JSON object.
fn json_object(request: &str, what: &str) -> Result<serde_json::Map<String, Value>, String> {
    match serde_json::from_str::<Value>(request) {
        Ok(Value::Object(object)) => Ok(object),
        Ok(_) => Err(format!("{what} must be a JSON object")),
        Err(error) => Err(format!("{what} is not JSON: {error}")),
    }
}

/// Opens a session. Until #2044 this validates the request and then answers
/// `not_wired` with a zero handle; no session is created.
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
        if matches!(
            kind,
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_RESUME
        ) && object.is_empty()
        {
            return fail(
                out_response,
                limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR,
                "resume payload cannot be empty",
            );
        }
        let operation = match kind {
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW => "open_new",
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_RESUME => "open_resume",
        };
        publish(out_response, not_wired_envelope(operation));
        limerick_mobile_status_t::LIMERICK_MOBILE_INTERNAL_ERROR
    })
}

/// Dispatches one JSON operation. No session can be open until #2044, so a
/// well-formed operation answers `not_wired`.
#[unsafe(no_mangle)]
pub extern "C" fn limerick_mobile_dispatch(
    handle: limerick_mobile_handle_t,
    operation_json: limerick_mobile_bytes_t,
    out_response: *mut limerick_mobile_owned_bytes_t,
) -> limerick_mobile_status_t {
    let _ = handle;
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
        let Some(op) = object.get("op").and_then(Value::as_str) else {
            return fail(
                out_response,
                limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR,
                "operation requires string field `op`",
            );
        };
        publish(out_response, not_wired_envelope(op));
        limerick_mobile_status_t::LIMERICK_MOBILE_INTERNAL_ERROR
    })
}

/// Closes a session. No handle is ever issued until #2044, so every handle is
/// invalid.
#[unsafe(no_mangle)]
pub extern "C" fn limerick_mobile_close(
    handle: limerick_mobile_handle_t,
) -> limerick_mobile_status_t {
    let _ = handle;
    limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_HANDLE
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
mod tests {
    use super::*;

    fn borrowed(value: &str) -> limerick_mobile_bytes_t {
        limerick_mobile_bytes_t {
            ptr: value.as_ptr(),
            len: value.len(),
        }
    }

    /// Copies an owned response into a JSON value and frees it.
    fn take(response: limerick_mobile_owned_bytes_t) -> Value {
        assert!(!response.ptr.is_null(), "response must carry an envelope");
        // SAFETY: the pointer and length came from `owned_bytes`.
        let bytes = unsafe { slice::from_raw_parts(response.ptr, response.len) }.to_vec();
        assert_eq!(
            limerick_mobile_owned_bytes_free(response),
            limerick_mobile_status_t::LIMERICK_MOBILE_OK
        );
        serde_json::from_slice(&bytes).expect("response is JSON")
    }

    fn open(
        kind: limerick_mobile_open_kind_t,
        request: &str,
    ) -> (limerick_mobile_status_t, u64, Value) {
        let mut handle = 99;
        let mut response = empty_owned();
        let status = limerick_mobile_open(kind, borrowed(request), &mut handle, &mut response);
        (status, handle, take(response))
    }

    fn dispatch(operation: &str) -> (limerick_mobile_status_t, Value) {
        let mut response = empty_owned();
        let status = limerick_mobile_dispatch(1, borrowed(operation), &mut response);
        (status, take(response))
    }

    fn assert_not_wired(envelope: &Value, operation: &str) {
        assert_eq!(envelope["ok"], false);
        let error = &envelope["error"];
        assert_eq!(error["code"], NOT_WIRED_CODE);
        assert_eq!(error["issue"], NOT_WIRED_ISSUE);
        assert_eq!(
            error["engine"]["save_format_version"], SAVE_FORMAT_VERSION,
            "the envelope reports the linked shared engine's save format"
        );
        let message = error["message"].as_str().expect("message");
        assert!(message.contains(operation), "{message}");
        assert!(message.contains("#2044"), "{message}");
    }

    #[test]
    fn open_new_answers_not_wired_without_a_handle() {
        let (status, handle, envelope) = open(
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
            r#"{"save_path":"/tmp/unused.sqlite"}"#,
        );
        assert_eq!(
            status,
            limerick_mobile_status_t::LIMERICK_MOBILE_INTERNAL_ERROR
        );
        assert_eq!(handle, 0, "no session is created");
        assert_not_wired(&envelope, "open_new");
    }

    #[test]
    fn open_resume_answers_not_wired() {
        let (status, handle, envelope) = open(
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_RESUME,
            r#"{"save_path":"/tmp/unused.sqlite"}"#,
        );
        assert_eq!(
            status,
            limerick_mobile_status_t::LIMERICK_MOBILE_INTERNAL_ERROR
        );
        assert_eq!(handle, 0);
        assert_not_wired(&envelope, "open_resume");
    }

    #[test]
    fn open_rejects_malformed_payloads_before_not_wired() {
        let cases = [
            (
                limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
                "not json",
            ),
            (limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW, "[]"),
            (
                limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_RESUME,
                "{}",
            ),
        ];
        for (kind, request) in cases {
            let (status, handle, envelope) = open(kind, request);
            assert_eq!(
                status,
                limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR,
                "{request}"
            );
            assert_eq!(handle, 0);
            assert_eq!(envelope["error"]["code"], "protocol_error", "{request}");
        }
    }

    #[test]
    fn open_rejects_invalid_utf8_and_oversized_payloads() {
        let invalid = [0xff_u8, 0xfe];
        let mut handle = 0;
        let mut response = empty_owned();
        let status = limerick_mobile_open(
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
            limerick_mobile_bytes_t {
                ptr: invalid.as_ptr(),
                len: invalid.len(),
            },
            &mut handle,
            &mut response,
        );
        assert_eq!(
            status,
            limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_UTF8
        );
        assert_eq!(take(response)["error"]["code"], "invalid_utf8");

        let oversized = "x".repeat(MAX_REQUEST_BYTES + 1);
        let (status, _, envelope) = open(
            limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
            &oversized,
        );
        assert_eq!(status, limerick_mobile_status_t::LIMERICK_MOBILE_TOO_LARGE);
        assert_eq!(envelope["error"]["code"], "too_large");
    }

    #[test]
    fn open_rejects_null_outputs() {
        let request = "{}";
        let mut response = empty_owned();
        assert_eq!(
            limerick_mobile_open(
                limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
                borrowed(request),
                ptr::null_mut(),
                &mut response,
            ),
            limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_ARGUMENT
        );
        let mut handle = 0;
        assert_eq!(
            limerick_mobile_open(
                limerick_mobile_open_kind_t::LIMERICK_MOBILE_OPEN_NEW,
                borrowed(request),
                &mut handle,
                ptr::null_mut(),
            ),
            limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_ARGUMENT
        );
    }

    #[test]
    fn dispatch_answers_not_wired_for_well_formed_operations() {
        let (status, envelope) = dispatch(r#"{"op":"submit","text":"hello"}"#);
        assert_eq!(
            status,
            limerick_mobile_status_t::LIMERICK_MOBILE_INTERNAL_ERROR
        );
        assert_not_wired(&envelope, "submit");
    }

    #[test]
    fn dispatch_rejects_malformed_operations() {
        for operation in ["", "nope", "[]", r#"{"text":"no op"}"#, r#"{"op":7}"#] {
            let (status, envelope) = dispatch(operation);
            assert_eq!(
                status,
                limerick_mobile_status_t::LIMERICK_MOBILE_PROTOCOL_ERROR,
                "{operation}"
            );
            assert_eq!(envelope["error"]["code"], "protocol_error", "{operation}");
        }
    }

    #[test]
    fn close_reports_every_handle_invalid() {
        assert_eq!(
            limerick_mobile_close(1),
            limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_HANDLE
        );
    }

    #[test]
    fn free_accepts_empty_and_rejects_dangling_lengths() {
        assert_eq!(
            limerick_mobile_owned_bytes_free(empty_owned()),
            limerick_mobile_status_t::LIMERICK_MOBILE_OK
        );
        assert_eq!(
            limerick_mobile_owned_bytes_free(limerick_mobile_owned_bytes_t {
                ptr: ptr::null_mut(),
                len: 4,
            }),
            limerick_mobile_status_t::LIMERICK_MOBILE_INVALID_ARGUMENT
        );
    }

    #[test]
    fn panics_are_contained_as_internal_errors() {
        assert_eq!(
            panic_contained(|| panic!("boom")),
            limerick_mobile_status_t::LIMERICK_MOBILE_INTERNAL_ERROR
        );
    }

    /// The Swift package vendors a copy of the C header next to its module
    /// map. The two copies must not drift.
    #[test]
    fn swift_bridge_header_matches_crate_header() {
        let crate_header = include_str!("../include/limerick_mobile_ffi.h");
        let bridge_header = include_str!(
            "../../../../mobile/RundaleBridge/Sources/LimerickMobileFFI/include/limerick_mobile_ffi.h"
        );
        assert_eq!(crate_header, bridge_header);
    }
}
