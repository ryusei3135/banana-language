#include "all.h"


NodeResult make_ok_result(long len) {
    NodeResult result = {
        .ok = len,
        .kind = Ok,
    };
    return result;
}

NodeResult make_err_result(char *msg) {
    NodeResult result = {
        .err = "\0",
        .kind = Err,
    };
    simd_strcpy(msg, result.err);
    return result;
}

