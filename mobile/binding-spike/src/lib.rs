#![deny(unsafe_op_in_unsafe_fn)]
#![allow(non_camel_case_types)]
// The C ABI intentionally remains callable as a safe C function after each
// exported pointer is checked for null. The unsafe operation is contained in
// the function body rather than exposed as an `unsafe` Swift import.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::slice;
use std::str;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

const MAX_INPUT_BYTES: usize = 4 * 1024;
const MAX_OUTPUT_BYTES: usize = 8 * 1024;
const MAX_BATCH_BYTES: usize = 64 * 1024;
const MAX_PENDING_EVENTS: usize = 512;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct rd_bytes_t {
    pub ptr: *const u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct rd_owned_bytes_t {
    pub ptr: *mut u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum rd_status_t {
    RD_STATUS_OK = 0,
    RD_STATUS_INVALID_ARGUMENT = 1,
    RD_STATUS_INVALID_UTF8 = 2,
    RD_STATUS_INVALID_HANDLE = 3,
    RD_STATUS_TOO_LARGE = 4,
    RD_STATUS_NOT_FOUND = 5,
    RD_STATUS_ALREADY_CANCELLED = 6,
    RD_STATUS_ALREADY_TERMINAL = 7,
    RD_STATUS_INTERNAL = 8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum rd_event_kind_t {
    RD_EVENT_PROVISIONAL = 1,
    RD_EVENT_COMPLETED = 2,
    RD_EVENT_LATE_IGNORED = 3,
}

pub type rd_session_handle_t = u64;
pub type rd_request_id_t = u64;
pub type rd_event_callback_t = Option<
    unsafe extern "C" fn(
        rd_session_handle_t,
        rd_request_id_t,
        rd_event_kind_t,
        rd_bytes_t,
        *mut c_void,
    ),
>;

#[derive(Clone, Copy)]
struct Callback {
    function: rd_event_callback_t,
    context: usize,
}

// The caller owns the callback context and promises it remains valid until a
// terminal callback or a drained dispose. The worker only copies the opaque
// address; it never dereferences it.
unsafe impl Send for Callback {}
unsafe impl Sync for Callback {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestState {
    Active,
    Canceled,
    Terminal,
}

struct Request {
    state: RequestState,
    callback: Option<Callback>,
}

struct Session {
    disposed: AtomicBool,
    next_request: AtomicU64,
    requests: Mutex<HashMap<rd_request_id_t, Request>>,
    events: Mutex<VecDeque<String>>,
    workers: Mutex<Vec<thread::JoinHandle<()>>>,
}

static SESSIONS: OnceLock<Mutex<HashMap<rd_session_handle_t, Arc<Session>>>> = OnceLock::new();
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

fn sessions() -> &'static Mutex<HashMap<rd_session_handle_t, Arc<Session>>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ffi<F>(function: F) -> rd_status_t
where
    F: FnOnce() -> rd_status_t,
{
    match catch_unwind(AssertUnwindSafe(function)) {
        Ok(status) => status,
        Err(_) => rd_status_t::RD_STATUS_INTERNAL,
    }
}

fn session_for(handle: rd_session_handle_t) -> Result<Arc<Session>, rd_status_t> {
    sessions()
        .lock()
        .expect("session registry lock poisoned")
        .get(&handle)
        .cloned()
        .ok_or(rd_status_t::RD_STATUS_INVALID_HANDLE)
}

fn read_utf8(bytes: rd_bytes_t, maximum: usize) -> Result<String, rd_status_t> {
    if bytes.len != 0 && bytes.ptr.is_null() {
        return Err(rd_status_t::RD_STATUS_INVALID_ARGUMENT);
    }
    if bytes.len > maximum {
        return Err(rd_status_t::RD_STATUS_TOO_LARGE);
    }
    if bytes.len == 0 {
        return Ok(String::new());
    }

    // SAFETY: the caller owns the borrowed bytes for this call. The null and
    // length checks above prevent constructing a slice from a null pointer.
    let raw = unsafe { slice::from_raw_parts(bytes.ptr, bytes.len) };
    str::from_utf8(raw)
        .map(str::to_owned)
        .map_err(|_| rd_status_t::RD_STATUS_INVALID_UTF8)
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                use std::fmt::Write;
                write!(escaped, "\\u{:04x}", character as u32).expect("String write");
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn accepted_event(request: rd_request_id_t, input: &str) -> String {
    format!(
        "{{\"kind\":\"accepted\",\"request_id\":{},\"input\":\"{}\"}}",
        request,
        json_escape(input)
    )
}

fn terminal_event(kind: &str, request: rd_request_id_t, output: &str) -> String {
    format!(
        "{{\"kind\":\"{}\",\"request_id\":{},\"output\":\"{}\"}}",
        kind,
        request,
        json_escape(output)
    )
}

fn canceled_event(request: rd_request_id_t) -> String {
    format!(
        "{{\"kind\":\"cancelled\",\"request_id\":{},\"committed\":false}}",
        request
    )
}

fn late_ignored_event(request: rd_request_id_t) -> String {
    format!(
        "{{\"kind\":\"late_result_ignored\",\"request_id\":{},\"committed\":false}}",
        request
    )
}

fn push_event(session: &Session, event: String) {
    let mut events = session.events.lock().expect("event queue lock poisoned");
    if events.len() < MAX_PENDING_EVENTS {
        events.push_back(event);
    }
}

fn invoke_callback(
    session: rd_session_handle_t,
    request: rd_request_id_t,
    kind: rd_event_kind_t,
    callback: Option<Callback>,
    payload: &str,
) {
    let Some(callback) = callback else {
        return;
    };
    let Some(function) = callback.function else {
        return;
    };
    let bytes = rd_bytes_t {
        ptr: payload.as_ptr(),
        len: payload.len(),
    };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: the callback and context were supplied by the caller and
        // the payload is borrowed only for the duration of this invocation.
        unsafe {
            function(
                session,
                request,
                kind,
                bytes,
                callback.context as *mut c_void,
            )
        };
    }));
}

fn create_request(
    session: &Arc<Session>,
    input: String,
    callback: Option<Callback>,
) -> Result<rd_request_id_t, rd_status_t> {
    if session.disposed.load(Ordering::Acquire) {
        return Err(rd_status_t::RD_STATUS_INVALID_HANDLE);
    }
    let request = session.next_request.fetch_add(1, Ordering::Relaxed);
    session
        .requests
        .lock()
        .expect("request lock poisoned")
        .insert(
            request,
            Request {
                state: RequestState::Active,
                callback,
            },
        );
    push_event(session, accepted_event(request, &input));
    Ok(request)
}

fn spawn_async_worker(
    session: Arc<Session>,
    session_handle: rd_session_handle_t,
    request: rd_request_id_t,
    input: String,
) {
    let worker_session = Arc::clone(&session);
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(15));
        if worker_session.disposed.load(Ordering::Acquire) {
            return;
        }

        let callback = worker_session
            .requests
            .lock()
            .expect("request lock poisoned")
            .get(&request)
            .and_then(|request| request.callback);
        let provisional = format!("{{\"input\":\"{}\"}}", json_escape(&input));
        invoke_callback(
            session_handle,
            request,
            rd_event_kind_t::RD_EVENT_PROVISIONAL,
            callback,
            &provisional,
        );

        // The delay leaves a deterministic window for Swift to cancel after
        // receiving provisional output but before a terminal response.
        thread::sleep(Duration::from_millis(120));
        if worker_session.disposed.load(Ordering::Acquire) {
            return;
        }

        let (state, callback) = {
            let mut requests = worker_session
                .requests
                .lock()
                .expect("request lock poisoned");
            let Some(request_state) = requests.get_mut(&request) else {
                return;
            };
            match request_state.state {
                RequestState::Active => {
                    request_state.state = RequestState::Terminal;
                    (RequestState::Terminal, request_state.callback)
                }
                RequestState::Canceled => (RequestState::Canceled, request_state.callback),
                RequestState::Terminal => (RequestState::Terminal, request_state.callback),
            }
        };

        match state {
            RequestState::Terminal => {
                let output = format!("echo:{}", input);
                push_event(
                    &worker_session,
                    terminal_event("completed", request, &output),
                );
                invoke_callback(
                    session_handle,
                    request,
                    rd_event_kind_t::RD_EVENT_COMPLETED,
                    callback,
                    &output,
                );
            }
            RequestState::Canceled => {
                // This models a provider result that arrived after Stop. It
                // is surfaced for diagnostics but is explicitly non-committing.
                push_event(&worker_session, late_ignored_event(request));
                invoke_callback(
                    session_handle,
                    request,
                    rd_event_kind_t::RD_EVENT_LATE_IGNORED,
                    callback,
                    "late result ignored after cancellation",
                );
            }
            RequestState::Active => unreachable!("active request is terminalized above"),
        }
    });
    session
        .workers
        .lock()
        .expect("worker lock poisoned")
        .push(worker);
}

fn owned_bytes(bytes: Vec<u8>) -> rd_owned_bytes_t {
    if bytes.is_empty() {
        return rd_owned_bytes_t {
            ptr: ptr::null_mut(),
            len: 0,
        };
    }
    let boxed = bytes.into_boxed_slice();
    let len = boxed.len();
    let ptr = Box::into_raw(boxed) as *mut u8;
    rd_owned_bytes_t { ptr, len }
}

#[unsafe(no_mangle)]
pub extern "C" fn rd_session_create(out_session: *mut rd_session_handle_t) -> rd_status_t {
    ffi(|| {
        if out_session.is_null() {
            return rd_status_t::RD_STATUS_INVALID_ARGUMENT;
        }
        let handle = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        let session = Arc::new(Session {
            disposed: AtomicBool::new(false),
            next_request: AtomicU64::new(1),
            requests: Mutex::new(HashMap::new()),
            events: Mutex::new(VecDeque::new()),
            workers: Mutex::new(Vec::new()),
        });
        sessions()
            .lock()
            .expect("session registry lock poisoned")
            .insert(handle, session);
        // SAFETY: the pointer was checked for null and points to caller-owned
        // storage for the duration of this call.
        unsafe { ptr::write(out_session, handle) };
        rd_status_t::RD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rd_session_dispose(session: rd_session_handle_t) -> rd_status_t {
    ffi(|| {
        let session = sessions()
            .lock()
            .expect("session registry lock poisoned")
            .remove(&session)
            .ok_or(rd_status_t::RD_STATUS_INVALID_HANDLE);
        let Ok(session) = session else {
            return rd_status_t::RD_STATUS_INVALID_HANDLE;
        };
        session.disposed.store(true, Ordering::Release);
        {
            let mut requests = session.requests.lock().expect("request lock poisoned");
            for request in requests.values_mut() {
                if request.state == RequestState::Active {
                    request.state = RequestState::Canceled;
                }
            }
        }
        let workers = std::mem::take(&mut *session.workers.lock().expect("worker lock poisoned"));
        for worker in workers {
            let _ = worker.join();
        }
        rd_status_t::RD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rd_session_submit_utf8(
    session: rd_session_handle_t,
    input: rd_bytes_t,
    out_request: *mut rd_request_id_t,
) -> rd_status_t {
    ffi(|| {
        if out_request.is_null() {
            return rd_status_t::RD_STATUS_INVALID_ARGUMENT;
        }
        let Ok(session_ref) = session_for(session) else {
            return rd_status_t::RD_STATUS_INVALID_HANDLE;
        };
        let input = match read_utf8(input, MAX_INPUT_BYTES) {
            Ok(input) => input,
            Err(status) => return status,
        };
        let Ok(request) = create_request(&session_ref, input, None) else {
            return rd_status_t::RD_STATUS_INVALID_HANDLE;
        };
        // SAFETY: the pointer was checked for null and points to caller-owned
        // storage for the duration of this call.
        unsafe { ptr::write(out_request, request) };
        rd_status_t::RD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rd_session_submit_async(
    session: rd_session_handle_t,
    input: rd_bytes_t,
    callback: rd_event_callback_t,
    context: *mut c_void,
    out_request: *mut rd_request_id_t,
) -> rd_status_t {
    ffi(|| {
        if out_request.is_null() || callback.is_none() {
            return rd_status_t::RD_STATUS_INVALID_ARGUMENT;
        }
        let Ok(session_ref) = session_for(session) else {
            return rd_status_t::RD_STATUS_INVALID_HANDLE;
        };
        let input = match read_utf8(input, MAX_INPUT_BYTES) {
            Ok(input) => input,
            Err(status) => return status,
        };
        let callback = Some(Callback {
            function: callback,
            context: context as usize,
        });
        let Ok(request) = create_request(&session_ref, input.clone(), callback) else {
            return rd_status_t::RD_STATUS_INVALID_HANDLE;
        };
        spawn_async_worker(session_ref, session, request, input);
        // SAFETY: the pointer was checked for null and points to caller-owned
        // storage for the duration of this call.
        unsafe { ptr::write(out_request, request) };
        rd_status_t::RD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rd_session_cancel(
    session: rd_session_handle_t,
    request: rd_request_id_t,
) -> rd_status_t {
    ffi(|| {
        let Ok(session_ref) = session_for(session) else {
            return rd_status_t::RD_STATUS_INVALID_HANDLE;
        };
        let transition = {
            let mut requests = session_ref.requests.lock().expect("request lock poisoned");
            let Some(request_state) = requests.get_mut(&request) else {
                return rd_status_t::RD_STATUS_NOT_FOUND;
            };
            match request_state.state {
                RequestState::Active => {
                    request_state.state = RequestState::Canceled;
                    (RequestState::Canceled, true)
                }
                RequestState::Canceled => (RequestState::Canceled, false),
                RequestState::Terminal => (RequestState::Terminal, false),
            }
        };
        match transition {
            (RequestState::Canceled, true) => {
                push_event(&session_ref, canceled_event(request));
                rd_status_t::RD_STATUS_OK
            }
            (RequestState::Canceled, false) => rd_status_t::RD_STATUS_ALREADY_CANCELLED,
            (RequestState::Terminal, false) => rd_status_t::RD_STATUS_ALREADY_TERMINAL,
            (RequestState::Active, _) | (RequestState::Terminal, true) => {
                unreachable!("cancel transition returned an impossible state")
            }
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rd_session_complete(
    session: rd_session_handle_t,
    request: rd_request_id_t,
    output: rd_bytes_t,
) -> rd_status_t {
    ffi(|| {
        let output = match read_utf8(output, MAX_OUTPUT_BYTES) {
            Ok(output) => output,
            Err(status) => return status,
        };
        let Ok(session_ref) = session_for(session) else {
            return rd_status_t::RD_STATUS_INVALID_HANDLE;
        };
        let transition = {
            let mut requests = session_ref.requests.lock().expect("request lock poisoned");
            let Some(request_state) = requests.get_mut(&request) else {
                return rd_status_t::RD_STATUS_NOT_FOUND;
            };
            match request_state.state {
                RequestState::Active => {
                    request_state.state = RequestState::Terminal;
                    (RequestState::Terminal, true)
                }
                RequestState::Canceled => (RequestState::Canceled, false),
                RequestState::Terminal => (RequestState::Terminal, false),
            }
        };
        match transition {
            (RequestState::Canceled, false) => rd_status_t::RD_STATUS_ALREADY_CANCELLED,
            (RequestState::Terminal, false) => rd_status_t::RD_STATUS_ALREADY_TERMINAL,
            (RequestState::Terminal, true) => {
                push_event(&session_ref, terminal_event("completed", request, &output));
                rd_status_t::RD_STATUS_OK
            }
            (RequestState::Canceled, true) | (RequestState::Active, _) => {
                unreachable!("complete transition returned an impossible state")
            }
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rd_session_take_json_batch(
    session: rd_session_handle_t,
    max_bytes: usize,
    out_batch: *mut rd_owned_bytes_t,
) -> rd_status_t {
    ffi(|| {
        if out_batch.is_null() || !(2..=MAX_BATCH_BYTES).contains(&max_bytes) {
            return rd_status_t::RD_STATUS_INVALID_ARGUMENT;
        }
        let Ok(session_ref) = session_for(session) else {
            return rd_status_t::RD_STATUS_INVALID_HANDLE;
        };
        let mut events = session_ref
            .events
            .lock()
            .expect("event queue lock poisoned");
        let mut selected = Vec::new();
        let mut size = 2usize;
        while let Some(event) = events.front() {
            let extra = event.len() + usize::from(!selected.is_empty());
            if size + extra > max_bytes {
                if selected.is_empty() {
                    return rd_status_t::RD_STATUS_TOO_LARGE;
                }
                break;
            }
            selected.push(events.pop_front().expect("event was present"));
            size += extra;
        }
        let body = selected.join(",");
        let result = owned_bytes(format!("[{}]", body).into_bytes());
        // SAFETY: the pointer was checked for null and points to caller-owned
        // storage for the duration of this call.
        unsafe { ptr::write(out_batch, result) };
        rd_status_t::RD_STATUS_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rd_owned_bytes_free(bytes: rd_owned_bytes_t) -> rd_status_t {
    ffi(|| {
        if bytes.len != 0 && bytes.ptr.is_null() {
            return rd_status_t::RD_STATUS_INVALID_ARGUMENT;
        }
        if bytes.ptr.is_null() {
            return rd_status_t::RD_STATUS_OK;
        }
        // SAFETY: the pointer/length pair came from owned_bytes and this
        // function is the single documented release operation.
        let slice = ptr::slice_from_raw_parts_mut(bytes.ptr, bytes.len);
        unsafe { drop(Box::from_raw(slice)) };
        rd_status_t::RD_STATUS_OK
    })
}
