#include "all.h"


NodeResult ok_val(long len) {
    NodeResult result = {
        .ok = len,
        .kind = Ok,
    };
    return result;
}

NodeResult make_err_result(char *msg) {
    int str_len = get_strlen(msg);
    NodeResult result = {
        .err = (char*)mem_malloc((long)str_len + 1),
        .kind = Err,
    };
    simd_strcpy(result.err, msg);
    return result;
}

void view_err_msg(NodeResult *val) {
    if (val->kind == Err)
        mem_free(val->err);
}