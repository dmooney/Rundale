#include "../../Sources/LimerickMobileFFI/include/limerick_mobile_ffi.h"

#include <stdlib.h>
#include <string.h>

static limerick_mobile_status_t stub_json(
    limerick_mobile_owned_bytes_t *out_response,
    const char *json
) {
    if (out_response == NULL || json == NULL) {
        return LIMERICK_MOBILE_INVALID_ARGUMENT;
    }
    size_t length = strlen(json);
    uint8_t *copy = (uint8_t *)malloc(length);
    if (copy == NULL && length != 0) {
        return LIMERICK_MOBILE_INTERNAL_ERROR;
    }
    if (length != 0) {
        memcpy(copy, json, length);
    }
    out_response->ptr = copy;
    out_response->len = length;
    return LIMERICK_MOBILE_OK;
}

limerick_mobile_status_t limerick_mobile_open(
    limerick_mobile_open_kind_t kind,
    limerick_mobile_bytes_t request_json,
    limerick_mobile_handle_t *out_handle,
    limerick_mobile_owned_bytes_t *out_response
) {
    (void)kind;
    (void)request_json;
    if (out_handle == NULL) {
        return LIMERICK_MOBILE_INVALID_ARGUMENT;
    }
    *out_handle = 1;
    return stub_json(out_response, "{\"ok\":true,\"value\":{}}");
}

limerick_mobile_status_t limerick_mobile_dispatch(
    limerick_mobile_handle_t handle,
    limerick_mobile_bytes_t operation_json,
    limerick_mobile_owned_bytes_t *out_response
) {
    (void)handle;
    (void)operation_json;
    return stub_json(out_response, "{\"ok\":true,\"value\":{}}");
}

limerick_mobile_status_t limerick_mobile_close(limerick_mobile_handle_t handle) {
    (void)handle;
    return LIMERICK_MOBILE_OK;
}

limerick_mobile_status_t limerick_mobile_owned_bytes_free(limerick_mobile_owned_bytes_t bytes) {
    free(bytes.ptr);
    return LIMERICK_MOBILE_OK;
}
