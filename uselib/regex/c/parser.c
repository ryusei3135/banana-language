#include "all.h"



CharOpt peek(Parser *this) {
    OpKind kind = Some;
    if (this->pos >= this->chars_len)
        kind = None;

    CharOpt opt = {
        kind == Some ? &this->chars[this->pos] : 0,
        kind
    };
    return opt;
}

static CharOpt peek2(Parser *this) {
    OpKind kind = Some;
    if (this->pos + 1 >= this->chars_len)
        kind = None;

    CharOpt opt = {
        kind == Some ? &this->chars[this->pos + 1] : 0,
        kind
    };
    return opt;
}

static CharOpt bump(Parser *this) {
    CharOpt c = peek(this);
    if (c.kind == Some) {
        this->pos += 1;
    }
    return c;
}

static char match_chr(Parser *this, char chr) {
    if (peek(this).kind == None)
        return 0;
    if (*peek(this).value == chr)
        return 1;
    return 0;
}

static char match_chr_2(Parser *this, char chr) {
    if (peek2(this).kind == None)
        return 0;
    if (*peek2(this).value == chr)
        return 1;
    return 0;
}

static char unmatch_bump(Parser *this, char chr) {
    if (peek(this).kind == None)
        return 1;
    if (*peek(this).value != chr)
        return 1;
    bump(this);
    return 0;
}


static NodeResult parse_concat(Parser *self, Nodes *nodes);
static NodeResult parse_bound(Parser *self, Nodes *nodes, long atom);
static NodeResult parse_atom(Parser *self, Nodes *nodes);
static NodeResult parse_repeat(Parser *self, Nodes *nodes);
static NodeResult parse_class(Parser *self, Nodes *nodes);
static NodeResult parse_escape(Parser *self, Nodes *nodes);
NodeResult parse_alt(Parser *self, Nodes *nodes);

#define ResultErrGen(msg)\
    CharResult result = {.err = (msg), Err};\
    return result;

#define ResultOkGen(c)\
    CharResult result = {.ok = (c), Ok};\
    return result;

#define ResultOK(T, c)\
    T result = {.ok = c, Ok};\
    return result;

#define PushNode(E)\
    NodeResult __r = E;\
    if (__r.kind == Err) {\
        view_err_msg(&__r);\
        return __r;\
    }\
    push_node(nodes, nodes->nodes[__r.ok]);

    
#define RetRangeNode(s, e) {\
    bump(self);\
    long __range = make_range_pair(nodes, s, e);\
    long __idx = make_node(nodes, Repeat, atom.ok, __range);\
    return ok_val(__idx); \
}


static CharResult parse_class_char(Parser *this) {
    CharOpt c0 = peek(this);
    if (c0.kind == None) {
        ResultErrGen("'[' に対応する ']' がありません");
    }

    if (*c0.value == '\\') {
        bump(this); // '\\' を消費
        if (peek(this).kind == None) {
            ResultErrGen("末尾がバックスラッシュで終わっています");
        }
        // change_byte_chr がエスケープされた文字自体の読み取り・消費を行う
        ResultOkGen(change_byte_chr(this));
    }

    bump(this); // 通常の文字を消費
    ResultOkGen(*c0.value); /* 修正: ポインタではなく文字値そのものを返す */
}


Parser parse_new(const char* pattern, long len) {
    static Parser parse;
    parse.chars = (char *)mem_malloc((long)len + 1);
    int copied = simd_strcpy(parse.chars, pattern);
    parse.chars_len = copied;
    parse.pos = 0;
    parse.group_count = 0;
    return parse;
}


NodeResult parse_alt(
    Parser *restrict self, 
    Nodes *restrict nodes
) {
    long start = nodes->len;
    int branch_count = 0;

    PushNode(parse_concat(self, nodes));
    branch_count++;

    while (match_chr(self, '|')) {
        bump(self);
        PushNode(parse_concat(self, nodes));
        branch_count++;
    }

    if (branch_count == 1)
        return ok_val(pop_node(nodes));
    else {
        long idx = make_range_pair(nodes, start, nodes->len);
        long alt_idx = make_alt_node(nodes, idx);
        return ok_val(alt_idx);
    }
}

/*
 * パターン文字列バッファの解放。
 * parse_alt は "(...)" グループごとに再帰されるため、内部で free() すると
 * 同じポインタを複数回解放してしまう (二重解放)。そのため解放はここに
 * 一本化し、一番外側の parse_alt 呼び出しが完全に終わった後に
 * 呼び出し元 (Rust 側の Regex::new) から一度だけ呼んでもらう。
 */
void parser_drop(Parser *self) {
    mem_free(self->chars);
    self->chars = 0;
}

static NodeResult parse_concat(
    Parser *restrict self, 
    Nodes *restrict nodes
) {
    long nodes_start = nodes->len;
    for (;;) {
        CharOpt r = peek(self);
        if (r.kind == None)
            break;
        if (*r.value == '|' || *r.value == ')')
            break;
        PushNode(parse_repeat(self, nodes));
    }
    long idx = make_range_pair(nodes, nodes_start, nodes->len);
    long concat_idx = make_concat_node(nodes, idx);
    return ok_val(concat_idx);
}


static NodeResult parse_repeat(
    Parser *restrict self, 
    Nodes *restrict nodes
) {
    NodeResult atom = parse_atom(self, nodes);
    if (atom.kind == Err)
        return atom;

    if (peek(self).kind == None)
        return ok_val(atom.ok);

    switch (*peek(self).value) {
    case '*':
        RetRangeNode(0, -1);
    case '+':
        RetRangeNode(1, -1);
    case '?':
        RetRangeNode(0, 1);
    case '{':
        return parse_bound(self, nodes, atom.ok);
    default:
        return atom;
    }
}

static NodeResult parse_bound(
    Parser *restrict self, 
    Nodes *restrict nodes, 
    long atom
) {
    long checkpoint = self->pos;
    bump(self); // '{'
    char* min_s[2] = {peek(self).value, 0};
    while (peek(self).kind == Some) {
        char *c = peek(self).value;
        if (is_byte_digit(*c) == 1) {
            min_s[1] = c;
            bump(self);
        } else
            break;
    }
    if (min_s[1] == 0) {
        self->pos = checkpoint + 1;
        long right = make_node(nodes, Char, (long)'{', 0);
        long range_idx = make_range_pair(nodes, atom, right);
        long idx = make_node(nodes, Range, range_idx, 0);
        return ok_val(idx);
    }

    char* max[2] = {(char*)0xFF, (char*)0xFF};
    struct { char* start; char* end; } max_s = {0, 0};

    if (match_chr(self, ',')) {
        max_s.start = peek(self).value;
        bump(self);
        while (peek(self).kind == Some) {
            char *c = peek(self).value;
            if (is_byte_digit(*c) == 1) {
                max_s.end = c;
                bump(self);
            } else
                break;
        }
        if (max_s.end != 0) {
            max[0] = max_s.start;
            max[1] = max_s.end;
        }
    } else {
        max[0] = min_s[0];
        max[1] = min_s[1];
    }

    if (unmatch_bump(self, '}'))
        return make_err_result("'{' に対応する '}' がありません\0");

    long idx = make_range_pair(
        nodes,
        parse_num(min_s[0], min_s[1]),
        parse_num(max[0], max[1])
    );
    return ok_val(
        make_node(nodes, Repeat, idx, 0)
    );
}


static NodeResult parse_atom(
    Parser *restrict self, 
    Nodes *restrict nodes
) {
    CharOpt c0 = bump(self);
    if (c0.kind == None)
        return make_err_result("パターンが予期せず終了しました\0");

    switch (*c0.value) {
    case '(': {
        int capturing = 1;
        if (match_chr(self, '?') && match_chr_2(self, ':')) {
            self->pos += 2;
            capturing = 0;
        }
        int group_idx = -1;
        if (capturing) {
            self->group_count += 1;
            group_idx = self->group_count;
        }
        NodeResult inner = parse_alt(self, nodes);
        if (inner.kind == Err)
            return inner;
        if (unmatch_bump(self, ')'))
            return make_err_result("'(' に対応する ')' がありません\0");
        if (group_idx != -1) {
            long node_idx = make_node(nodes, Group, inner.ok, group_idx);
            return ok_val(node_idx);
        } else
            return inner;
    }
    case '.': {
        long idx = make_one_node(nodes, Any);
        return ok_val(idx);
    }
    case '^': {
        long idx = make_one_node(nodes, Start);
        return ok_val(idx);
    }
    case '$': {
        long idx = make_one_node(nodes, End);
        return ok_val(idx);
    }
    case '[':
        return parse_class(self, nodes);
    case '\\':
        return parse_escape(self, nodes);
    default: {
        long idx = make_node(nodes, Char, (long)*c0.value, 0);
        return ok_val(idx);
    }
    }
}

static long make_cls_pairs(
    Nodes *nodes, 
    const char pairs[][2], 
    int count, 
    int negated
) {
    long start = nodes->len;
    for (int i = 0; i < count; i++) {
        make_range_pair(nodes, pairs[i][0], pairs[i][1]);
    }
    long range_idx = make_range_pair(nodes, start, nodes->len);
    return make_node(nodes, Class, range_idx, negated);
}

static void gen_range_pairs(Nodes *nodes, char c);

static NodeResult parse_escape(
    Parser *restrict self, 
    Nodes *restrict nodes
) {
    if (peek(self).kind == None)
        return make_err_result("末尾がバックスラッシュで終わっています\0");

    char c = *peek(self).value;
    bump(self);

    switch (c) {
    case 'd': case 'D': {
        char (*table)[2] = (char (*)[2])shorthand_class_ranges('d');
        int count = (unsigned char)table[0][0];
        long idx = make_cls_pairs(nodes, table + 1, count, c == 'D');
        return ok_val(idx);
    }
    case 'w': case 'W': {
        char (*table)[2] = (char (*)[2])shorthand_class_ranges('w');
        int count = (unsigned char)table[0][0];
        long idx = make_cls_pairs(nodes, table + 1, count, c == 'W');
        return ok_val(idx);
    }
    case 's': case 'S': {
        char (*table)[2] = (char (*)[2])shorthand_class_ranges('s');
        int count = (unsigned char)table[0][0];
        long idx = make_cls_pairs(nodes, table + 1, count, c == 'S');
        return ok_val(idx);
    }
    case 'n':
        return ok_val(make_node(nodes, Char, (long)'\n', 0));
    case 't':
        return ok_val(make_node(nodes, Char, (long)'\t', 0));
    case 'r':
        return ok_val(make_node(nodes, Char, (long)'\r', 0));
    default:
        return ok_val(make_node(nodes, Char, (long)c, 0));
    }
}

static NodeResult parse_class(
    Parser *restrict self, 
    Nodes *restrict nodes
) {
    int negated = 0;
    if (negated=match_chr(self, '^'))
        bump(self);

    long ranges_start = nodes->len;
    int first = 1;

    for (;;) {
        if (match_chr(self, ']') && !first) {
            bump(self);
            break;
        } else if (peek(self).kind == None)
            return make_err_result("'[' に対応する ']' がありません\0");

        first = 0;

        CharOpt result = peek2(self);
        char c = *result.value;
        if (match_chr(self, '\\')
            && result.kind == Some
            && (
                c == 'd' 
                || c == 'w' 
                || c == 's'
            ))
        {
            bump(self);
            char kind = *bump(self).value;
            switch (kind) {
            case 'd':
                make_range_pair(nodes, '0', '9');
                break;
            case 'w':
                gen_range_pairs(nodes, 'w');
                break;
            case 's':
                gen_range_pairs(nodes, 's');
                break;
            default:
                continue;
            }
            continue;
        }

        CharResult r1 = parse_class_char(self);
        if (r1.kind == Err) {
            NodeResult err;
            simd_strcpy(err.err, r1.err);
            err.kind = Err;
            return err;
        }
        char c1 = r1.ok;

        if (match_chr(self, '-')
            && peek2(self).kind == Some
            && *peek2(self).value != ']')
        {
            bump(self);
            CharResult r2 = parse_class_char(self);
            if (r2.kind == Err) {
                NodeResult err;
                simd_strcpy(err.err, r2.err);
                err.kind = Err;
                return err;
            }
            make_range_pair(nodes, c1, r2.ok);
        } else
            make_range_pair(nodes, c1, c1);
    }

    long range_idx = make_range_pair(nodes, ranges_start, nodes->len);
    long idx = make_node(nodes, Class, range_idx, negated);
    return ok_val(idx);
}

static void gen_range_pairs(Nodes *nodes, char c) {
    char (*pair)[2] = (char (*)[2])shorthand_class_ranges(c);
    int len = (unsigned char)pair[0][0];
    for (int i = 1; i <= len; i++) {
        make_range_pair(nodes, pair[i][0], pair[i][1]);
    }
}

