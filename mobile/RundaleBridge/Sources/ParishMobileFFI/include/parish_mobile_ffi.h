#ifndef PARISH_MOBILE_FFI_H
#define PARISH_MOBILE_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define PARISH_MOBILE_ABI_VERSION 1u

typedef uint64_t parish_mobile_handle_t;

/* Borrowed for the duration of one call. The Rust side copies valid UTF-8. */
typedef struct parish_mobile_bytes {
    const uint8_t *ptr;
    size_t len;
} parish_mobile_bytes_t;

/* Owned by Rust until parish_mobile_owned_bytes_free is called exactly once. */
typedef struct parish_mobile_owned_bytes {
    uint8_t *ptr;
    size_t len;
} parish_mobile_owned_bytes_t;

typedef enum parish_mobile_status {
    PARISH_MOBILE_OK = 0,
    PARISH_MOBILE_INVALID_ARGUMENT = 1,
    PARISH_MOBILE_INVALID_UTF8 = 2,
    PARISH_MOBILE_INVALID_HANDLE = 3,
    PARISH_MOBILE_TOO_LARGE = 4,
    PARISH_MOBILE_PROTOCOL_ERROR = 5,
    PARISH_MOBILE_CLOSED = 6,
    PARISH_MOBILE_INTERNAL_ERROR = 7,
} parish_mobile_status_t;

typedef enum parish_mobile_open_kind {
    PARISH_MOBILE_OPEN_NEW = 1,
    PARISH_MOBILE_OPEN_RESUME = 2,
} parish_mobile_open_kind_t;

/*
 * Open returns a session token plus a JSON presentation snapshot/envelope.
 * The options/resume JSON is copied during the call. No Rust pointer survives
 * the call except the opaque token, which must be closed exactly once.
 */
parish_mobile_status_t parish_mobile_open(
    parish_mobile_open_kind_t kind,
    parish_mobile_bytes_t request_json,
    parish_mobile_handle_t *out_handle,
    parish_mobile_owned_bytes_t *out_response
);

/*
 * Dispatch one bounded JSON operation through the serialized session owner.
 * The response is an owned JSON envelope and is never a borrowed engine value.
 */
parish_mobile_status_t parish_mobile_dispatch(
    parish_mobile_handle_t handle,
    parish_mobile_bytes_t operation_json,
    parish_mobile_owned_bytes_t *out_response
);

/* Close drains the session's mutation lane before invalidating the token. */
parish_mobile_status_t parish_mobile_close(parish_mobile_handle_t handle);

/* Releases one response returned by parish_mobile_open/dispatch. */
parish_mobile_status_t parish_mobile_owned_bytes_free(parish_mobile_owned_bytes_t bytes);

#ifdef __cplusplus
}
#endif

#endif
