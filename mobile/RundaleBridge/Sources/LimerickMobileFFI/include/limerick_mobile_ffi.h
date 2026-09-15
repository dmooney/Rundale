#ifndef LIMERICK_MOBILE_FFI_H
#define LIMERICK_MOBILE_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define LIMERICK_MOBILE_ABI_VERSION 1u

typedef uint64_t limerick_mobile_handle_t;

/* Borrowed for the duration of one call. The Rust side copies valid UTF-8. */
typedef struct limerick_mobile_bytes {
    const uint8_t *ptr;
    size_t len;
} limerick_mobile_bytes_t;

/* Owned by Rust until limerick_mobile_owned_bytes_free is called exactly once. */
typedef struct limerick_mobile_owned_bytes {
    uint8_t *ptr;
    size_t len;
} limerick_mobile_owned_bytes_t;

typedef enum limerick_mobile_status {
    LIMERICK_MOBILE_OK = 0,
    LIMERICK_MOBILE_INVALID_ARGUMENT = 1,
    LIMERICK_MOBILE_INVALID_UTF8 = 2,
    LIMERICK_MOBILE_INVALID_HANDLE = 3,
    LIMERICK_MOBILE_TOO_LARGE = 4,
    LIMERICK_MOBILE_PROTOCOL_ERROR = 5,
    LIMERICK_MOBILE_CLOSED = 6,
    LIMERICK_MOBILE_INTERNAL_ERROR = 7,
} limerick_mobile_status_t;

typedef enum limerick_mobile_open_kind {
    LIMERICK_MOBILE_OPEN_NEW = 1,
    LIMERICK_MOBILE_OPEN_RESUME = 2,
} limerick_mobile_open_kind_t;

/*
 * Open returns a session token plus a JSON presentation snapshot/envelope.
 * The options/resume JSON is copied during the call. No Rust pointer survives
 * the call except the opaque token, which must be closed exactly once.
 */
limerick_mobile_status_t limerick_mobile_open(
    limerick_mobile_open_kind_t kind,
    limerick_mobile_bytes_t request_json,
    limerick_mobile_handle_t *out_handle,
    limerick_mobile_owned_bytes_t *out_response
);

/*
 * Dispatch one bounded JSON operation through the serialized session owner.
 * The response is an owned JSON envelope and is never a borrowed engine value.
 */
limerick_mobile_status_t limerick_mobile_dispatch(
    limerick_mobile_handle_t handle,
    limerick_mobile_bytes_t operation_json,
    limerick_mobile_owned_bytes_t *out_response
);

/* Close drains the session's mutation lane before invalidating the token. */
limerick_mobile_status_t limerick_mobile_close(limerick_mobile_handle_t handle);

/* Releases one response returned by limerick_mobile_open/dispatch. */
limerick_mobile_status_t limerick_mobile_owned_bytes_free(limerick_mobile_owned_bytes_t bytes);

#ifdef __cplusplus
}
#endif

#endif
