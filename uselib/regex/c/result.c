#include "all.h"


NodeResult ok_val(long len) {
    NodeResult result = {
        .ok = len,
        .kind = Ok,
    };
    return result;
}

NodeResult make_err_result(char *msg) {
    NodeResult result = {
        .err = {0},
        .kind = Err,
    };
    simd_strcpy(result.err, msg);
    return result;
}

