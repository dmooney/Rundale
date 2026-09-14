#include "../../Sources/ParishMobileFFI/include/parish_mobile_ffi.h"

#include <stdlib.h>
#include <string.h>

static parish_mobile_status_t stub_json(
    parish_mobile_owned_bytes_t *out_response,
    const char *json
) {
    if (out_response == NULL || json == NULL) {
        return PARISH_MOBILE_INVALID_ARGUMENT;
    }
    size_t length = strlen(json);
    uint8_t *copy = (uint8_t *)malloc(length);
    if (copy == NULL && length != 0) {
        return PARISH_MOBILE_INTERNAL_ERROR;
    }
    if (length != 0) {
        memcpy(copy, json, length);
    }
    out_response->ptr = copy;
    out_response->len = length;
    return PARISH_MOBILE_OK;
}

parish_mobile_status_t parish_mobile_open(
    parish_mobile_open_kind_t kind,
    parish_mobile_bytes_t request_json,
    parish_mobile_handle_t *out_handle,
    parish_mobile_owned_bytes_t *out_response
) {
    (void)kind;
    (void)request_json;
    if (out_handle == NULL) {
        return PARISH_MOBILE_INVALID_ARGUMENT;
    }
    *out_handle = 1;
    return stub_json(out_response, "{\"ok\":true,\"value\":{}}");
}

parish_mobile_status_t parish_mobile_dispatch(
    parish_mobile_handle_t handle,
    parish_mobile_bytes_t operation_json,
    parish_mobile_owned_bytes_t *out_response
) {
    (void)handle;
    (void)operation_json;
    return stub_json(out_response, "{\"ok\":true,\"value\":{}}");
}

parish_mobile_status_t parish_mobile_close(parish_mobile_handle_t handle) {
    (void)handle;
    return PARISH_MOBILE_OK;
}

parish_mobile_status_t parish_mobile_owned_bytes_free(parish_mobile_owned_bytes_t bytes) {
    free(bytes.ptr);
    return PARISH_MOBILE_OK;
}
