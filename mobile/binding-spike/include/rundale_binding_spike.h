#ifndef RUNDALE_BINDING_SPIKE_H
#define RUNDALE_BINDING_SPIKE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef uint64_t rd_session_handle_t;
typedef uint64_t rd_request_id_t;

/* Borrowed for the duration of a call or callback. */
typedef struct rd_bytes {
    const uint8_t *ptr;
    size_t len;
} rd_bytes_t;

/* Owned by Rust until rd_owned_bytes_free is called exactly once. */
typedef struct rd_owned_bytes {
    uint8_t *ptr;
    size_t len;
} rd_owned_bytes_t;

typedef enum rd_status {
    RD_STATUS_OK = 0,
    RD_STATUS_INVALID_ARGUMENT = 1,
    RD_STATUS_INVALID_UTF8 = 2,
    RD_STATUS_INVALID_HANDLE = 3,
    RD_STATUS_TOO_LARGE = 4,
    RD_STATUS_NOT_FOUND = 5,
    RD_STATUS_ALREADY_CANCELLED = 6,
    RD_STATUS_ALREADY_TERMINAL = 7,
    RD_STATUS_INTERNAL = 8,
} rd_status_t;

typedef enum rd_event_kind {
    RD_EVENT_PROVISIONAL = 1,
    RD_EVENT_COMPLETED = 2,
    RD_EVENT_LATE_IGNORED = 3,
} rd_event_kind_t;

typedef void (*rd_event_callback_t)(
    rd_session_handle_t session,
    rd_request_id_t request,
    rd_event_kind_t kind,
    rd_bytes_t payload,
    void *context
);

/*
 * All exported functions are panic-contained. Rust never transfers a
 * Rust-owned pointer or String into Swift; every returned buffer is freed by
 * rd_owned_bytes_free. Callback payload memory is valid only during the call.
 */
rd_status_t rd_session_create(rd_session_handle_t *out_session);
rd_status_t rd_session_dispose(rd_session_handle_t session);

rd_status_t rd_session_submit_utf8(
    rd_session_handle_t session,
    rd_bytes_t input,
    rd_request_id_t *out_request
);

rd_status_t rd_session_submit_async(
    rd_session_handle_t session,
    rd_bytes_t input,
    rd_event_callback_t callback,
    void *context,
    rd_request_id_t *out_request
);

rd_status_t rd_session_cancel(
    rd_session_handle_t session,
    rd_request_id_t request
);

/* Models a terminal result arriving after the local cancellation decision. */
rd_status_t rd_session_complete(
    rd_session_handle_t session,
    rd_request_id_t request,
    rd_bytes_t output
);

/* Returns a JSON array no larger than max_bytes, including its brackets. */
rd_status_t rd_session_take_json_batch(
    rd_session_handle_t session,
    size_t max_bytes,
    rd_owned_bytes_t *out_batch
);

rd_status_t rd_owned_bytes_free(rd_owned_bytes_t bytes);

#ifdef __cplusplus
}
#endif

#endif
