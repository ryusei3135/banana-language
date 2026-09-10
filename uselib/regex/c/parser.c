#include "all.h"


/* ============================================================
 * Parser utility
 * ============================================================ */

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
    if (c.kind == Some)
        this->pos += 1;
    return c;
}


static char match_chr(
    Parser *this,
    char chr
) {
    if (peek(this).kind == None)
        return 0;
    return *peek(this).value == chr;
}


static char match_chr_2(
    Parser *this,
    char chr
) {
    if (peek2(this).kind == None)
        return 0;
    return *peek2(this).value == chr;
}


static char unmatch_bump(
    Parser *this,
    char chr
) {
    if (peek(this).kind == None)
        return 1;
    if (*peek(this).value != chr)
        return 1;
    bump(this);
    return 0;
}


/* ============================================================
 * Result macros
 * ============================================================ */

#define ResultErrGen(msg)\
    CharResult result = {.err = (msg), Err};\
    return result;


#define ResultOkGen(c)\
    CharResult result = {.ok = (c), Ok};\
    return result;


#define ResultOK(T, c)\
    T result = {.ok = c, Ok};\
    return result;

static void gen_range_pairs(Nodes *, char);
static NodeResult parse_class(Parser *restrict, Nodes *restrict);
static NodeResult parse_atom(Parser *restrict, Nodes *restrict);
static NodeResult parse_repeat(Parser *restrict, Nodes *restrict);
static NodeResult parse_bound(Parser *restrict, Nodes *restrict, long);
static NodeResult parse_escape(Parser *restrict, Nodes *restrict);
/* ============================================================
 * Node helper
 * ============================================================ */

/*
 * parse_* が返した Node index の Node を
 * concat / alt の要素として追加する。
 */
/*
 * 修正: 以前はエラー時にここで view_err_msg() を呼びメッセージを
 * 解放してから呼び出し元へ返していたが、これは「エラーを最終的に
 * 消費する場所」ではなく「エラーを一段上に伝播するだけの場所」であり、
 * 一度解放したポインタがそのまま Rust 側 (cstr_to_string) まで
 * 伝わって use-after-free になっていた。さらに parse_alt のように
 * 複数階層ネストする呼び出し (グループ `(...)` の再帰解析) では、
 * 一段深いところで解放済みのポインタを上の階層で再度解放しようとして
 * 二重解放を引き起こしていた。ここでは解放せずそのまま返す
 * (parse_repeat や parse_atom のグループ処理など、他の伝播箇所と
 * 同じ扱いに揃える)。
 */
#define PushNode(E)\
    NodeResult __r = E;\
    if (__r.kind == Err) {\
        return __r;\
    }\
    push_node(nodes, nodes->nodes[__r.ok]);


/*
 * atom + quantifier
 *
 * *
 * +
 * ?
 */
#define RetRangeNode(s, e) {\
    bump(self);\
    long __range = make_range_pair(nodes, s, e);\
    long __idx = make_node(nodes, Repeat, atom.ok, __range);\
    return ok_val(__idx);\
}


/* ============================================================
 * Character inside []
 * ============================================================ */

static CharResult parse_class_char(Parser *this) {
    CharOpt c0 = peek(this);
    if (c0.kind == None) {
        ResultErrGen(
            "'[' に対応する ']' がありません"
        );
    }
    if (*c0.value == '\\') {
        bump(this);
        if (peek(this).kind == None) {
            ResultErrGen(
                "末尾がバックスラッシュで終わっています"
            );
        }
        ResultOkGen(
            change_byte_chr(this)
        );
    }
    bump(this);
    ResultOkGen(*c0.value);
}


/* ============================================================
 * Parser creation
 * ============================================================ */

Parser *parse_new(const char *pattern, long len) {
    Parser *parser = mem_malloc(sizeof(Parser));
    if (parser == 0)
        return 0;
    parser->pos = 0;
    parser->group_count = 0;
    parser->chars = mem_malloc(len + 1);
    if (parser->chars == 0) {
        mem_free(parser);
        return 0;
    }
    for (long i = 0; i <= len; i++)
        parser->chars[i] = pattern[i];
    parser->chars_len = len;
    return parser;
}


/*
 * parse_new() が mem_malloc() で確保した Parser を解放する。
 * chars バッファ -> Parser 本体、の順で解放する。
 */
void parser_drop(Parser *self) {
    if (self == 0)
        return;
    mem_free(self->chars);
    mem_free(self);
}


/* ============================================================
 * CONCAT
 * ============================================================ */

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
        PushNode(
            parse_repeat(self, nodes)
        );
    }
    long idx =
        make_range_pair(
            nodes,
            nodes_start,
            nodes->len
        );
    long concat_idx =
        make_concat_node(
            nodes,
            idx
        );
    return ok_val(concat_idx);
}


/* ============================================================
 * ALT
 *
 * concat ('|' concat)*
 * ============================================================ */

NodeResult parse_alt(
    Parser *restrict self,
    Nodes *restrict nodes
) {
    long nodes_start = nodes->len;
    /*
     * ここでは PushNode マクロを使わない。
     * PushNode はエラー時に view_err_msg() でメッセージを解放してから
     * 呼び出し元へ返す設計になっており、これは「エラーを一度だけ消費する」
     * 末端の呼び出し元 (例: parse_concat が parse_repeat を包む場合) を
     * 想定したものである。parse_alt はグループ `(...)` の中身を再帰的に
     * 解析するために複数階層ネストしうるため、PushNode を重ねて使うと
     * 一段深いところで既に解放済みのエラーメッセージを、上の階層の
     * PushNode がもう一度 view_err_msg() で解放しようとして二重解放になる。
     * parse_atom の '(' ケースで parse_alt の結果を素通しする書き方と
     * 同様に、ここでも解放せずそのまま返す。
     */
    NodeResult first =
        parse_concat(self, nodes);
    if (first.kind == Err)
        return first;
    push_node(nodes, nodes->nodes[first.ok]);
    while (match_chr(self, '|')) {
        bump(self);
        NodeResult next =
            parse_concat(self, nodes);
        if (next.kind == Err)
            return next;
        push_node(nodes, nodes->nodes[next.ok]);
    }
    long idx =
        make_range_pair(
            nodes,
            nodes_start,
            nodes->len
        );
    long alt_idx =
        make_alt_node(
            nodes,
            idx
        );
    return ok_val(alt_idx);
}


/* ============================================================
 * REPEAT
 * ============================================================ */

static NodeResult parse_repeat(
    Parser *restrict self,
    Nodes *restrict nodes
) {
    NodeResult atom =
        parse_atom(self, nodes);
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
            return parse_bound(
                self,
                nodes,
                atom.ok
            );
        default:
            return atom;
    }
}


/* ============================================================
 * {n}, {n,m}, {n,}
 * ============================================================ */

static NodeResult parse_bound(
    Parser *restrict self,
    Nodes *restrict nodes,
    long atom
) {
    long checkpoint = self->pos;
    bump(self);
    char *min_s[2] = {
        peek(self).value,
        0
    };
    while (peek(self).kind == Some) {
        char *c =
            peek(self).value;
        if (is_byte_digit(*c) == 1) {
            min_s[1] = c;
            bump(self);
        } else
            break;
    }
    if (min_s[1] == 0) {
        self->pos =
            checkpoint + 1;
        long right =
            make_node(
                nodes,
                Char,
                (long)'{',
                0
            );
        long range_idx =
            make_range_pair(
                nodes,
                atom,
                right
            );
        long idx =
            make_node(
                nodes,
                Range,
                range_idx,
                0
            );
        return ok_val(idx);
    }
    char *max[2] = {
        (char*)0xFF,
        (char*)0xFF
    };
    struct {
        char *start;
        char *end;
    } max_s = {
        0,
        0
    };
    if (match_chr(self, ',')) {
        max_s.start =
            peek(self).value;
        bump(self);
        while (peek(self).kind == Some) {
            char *c =
                peek(self).value;
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
        return make_err_result(
            "'{' に対応する '}' がありません\0"
        );
    long idx =
        make_range_pair(
            nodes,
            parse_num(
                min_s[0],
                min_s[1]
            ),
            parse_num(
                max[0],
                max[1]
            )
        );
    return ok_val(
        make_node(
            nodes,
            Repeat,
            idx,
            0
        )
    );
}


/* ============================================================
 * ATOM
 * ============================================================ */

static NodeResult parse_atom(
    Parser *restrict self,
    Nodes *restrict nodes
) {
    CharOpt c0 =
        bump(self);
    if (c0.kind == None)
        return make_err_result(
            "パターンが予期せず終了しました\0"
        );
    switch (*c0.value) {
        case '(':
        {
            int capturing = 1;
            if (
                match_chr(self, '?')
                &&
                match_chr_2(self, ':')
            ) {
                self->pos += 2;
                capturing = 0;
            }
            int group_idx = -1;
            if (capturing) {
                self->group_count += 1;
                group_idx =
                    self->group_count;
            }
            NodeResult inner =
                parse_alt(
                    self,
                    nodes
                );
            if (inner.kind == Err)
                return inner;
            if (unmatch_bump(self,')'))
                return make_err_result(
                    "'(' に対応する ')' がありません\0"
                );
            if (group_idx != -1) {
                long node_idx =
                    make_node(
                        nodes,
                        Group,
                        inner.ok,
                        group_idx
                    );
                return ok_val(node_idx);
            } else
                return inner;
        }
        case '.':
        {
            long idx =
                make_one_node(
                    nodes,
                    Any
                );
            return ok_val(idx);
        }
        case '^':
        {
            long idx =
                make_one_node(
                    nodes,
                    Start
                );
            return ok_val(idx);
        }
        case '$':
        {
            long idx =
                make_one_node(
                    nodes,
                    End
                );
            return ok_val(idx);
        }
        case '[':
            return parse_class(
                self,
                nodes
            );
        case '\\':
            return parse_escape(
                self,
                nodes
            );
        default:
        {
            long idx =
                make_node(
                    nodes,
                    Char,
                    (long)*c0.value,
                    0
                );
            return ok_val(idx);
        }
    }
}

/* ============================================================
 * Character class helper
 * ============================================================ */

static long make_cls_pairs(
    Nodes *nodes,
    const char pairs[][2],
    int count,
    int negated
) {
    long start =
        nodes->len;
    for (int i = 0; i < count; i++)
        make_range_pair(
            nodes,
            pairs[i][0],
            pairs[i][1]
        );
    long range_idx =
        make_range_pair(
            nodes,
            start,
            nodes->len
        );
    return make_node(
        nodes,
        Class,
        range_idx,
        negated
    );
}


/* ============================================================
 * Escape
 * ============================================================ */

static NodeResult parse_escape(
    Parser *restrict self,
    Nodes *restrict nodes
) {
    if (peek(self).kind == None)
        return make_err_result(
            "末尾がバックスラッシュで終わっています\0"
        );
    char c =
        *peek(self).value;
    bump(self);
    switch (c) {
        case 'd':
        case 'D':
        {
            char (*table)[2] =
                (char (*)[2])
                shorthand_class_ranges('d');
            int count =
                (unsigned char)table[0][0];
            long idx =
                make_cls_pairs(
                    nodes,
                    table + 1,
                    count,
                    c == 'D'
                );
            return ok_val(idx);
        }
        case 'w':
        case 'W':
        {
            char (*table)[2] =
                (char (*)[2])
                shorthand_class_ranges('w');
            int count =
                (unsigned char)table[0][0];
            long idx =
                make_cls_pairs(
                    nodes,
                    table + 1,
                    count,
                    c == 'W'
                );
            return ok_val(idx);
        }
        case 's':
        case 'S':
        {
            char (*table)[2] =
                (char (*)[2])
                shorthand_class_ranges('s');
            int count =
                (unsigned char)table[0][0];
            long idx =
                make_cls_pairs(
                    nodes,
                    table + 1,
                    count,
                    c == 'S'
                );
            return ok_val(idx);
        }
        case 'n':
            return ok_val(
                make_node(
                    nodes,
                    Char,
                    (long)'\n',
                    0
                )
            );
        case 't':
            return ok_val(
                make_node(
                    nodes,
                    Char,
                    (long)'\t',
                    0
                )
            );
        case 'r':
            return ok_val(
                make_node(
                    nodes,
                    Char,
                    (long)'\r',
                    0
                )
            );
        default:
            return ok_val(
                make_node(
                    nodes,
                    Char,
                    (long)c,
                    0
                )
            );
    }
}


/* ============================================================
 * Character class
 * ============================================================ */

static NodeResult parse_class(
    Parser *restrict self,
    Nodes *restrict nodes
) {
    int negated = 0;
    if (unmatch_bump(self,'['))
        return make_err_result("'[' がありません\0");
    if (match_chr(self, '^')) {
        bump(self);
        negated = 1;
    }
    long ranges_start =
        nodes->len;
    while (peek(self).kind == Some) {
        if (*peek(self).value == ']') {
            bump(self);
            break;
        }
        CharOpt result =
            peek2(self);
        char c =
            result.kind == Some
                ? *result.value
                : 0;
        if (
            match_chr(self, '\\')
            &&
            result.kind == Some
            &&
            (
                c == 'd'
                ||
                c == 'w'
                ||
                c == 's'
                ||
                c == 'D'
                ||
                c == 'W'
                ||
                c == 'S'
            )
        ) {
            bump(self);
            char kind =
                *bump(self).value;
            switch (kind) {
                case 'd':
                    make_range_pair(
                        nodes,
                        '0',
                        '9'
                    );
                    break;
                case 'w':
                    gen_range_pairs(
                        nodes,
                        'w'
                    );
                    break;
                case 's':
                    gen_range_pairs(
                        nodes,
                        's'
                    );
                    break;
                case 'D':
                    gen_range_pairs(
                        nodes,
                        'D'
                    );
                    break;
                case 'W':
                    gen_range_pairs(
                        nodes,
                        'W'
                    );
                    break;
                case 'S':
                    gen_range_pairs(
                        nodes,
                        'S'
                    );
                    break;
            }
            continue;
        }
        CharResult c1 =
            parse_class_char(self);
        if (c1.kind == Err)
            return make_err_result(c1.err);
        if (
            match_chr(self, '-')
            &&
            peek2(self).kind == Some
        ) {
            bump(self);
            CharResult r2 =
                parse_class_char(self);
            if (r2.kind == Err)
                return make_err_result(r2.err);
            make_range_pair(nodes,c1.ok,r2.ok);
        } else
            make_range_pair(nodes,c1.ok,c1.ok);
    }
    long range_idx =
        make_range_pair(
            nodes,
            ranges_start,
            nodes->len
        );
    long idx =
        make_node(
            nodes,
            Class,
            range_idx,
            negated
        );
    return ok_val(idx);
}
/* ============================================================
 * Shorthand range
 * ============================================================ */

static void gen_range_pairs(
    Nodes *nodes,
    char c
) {
    switch (c) {
        case 'w': {
            char (*range)[2] = shorthand_class_ranges('w');
            for (int i=1; i < range[0][0];i++)
                make_range_pair(nodes, range[i][0], range[i][1]);
            break;
        }
        case 's': {
            char (*range)[2] = shorthand_class_ranges('s');
            for (int i=1; i < range[0][0];i++)
                make_range_pair(nodes, range[i][0], range[i][1]);
            break;
        }
        default:
            break;
    }
}
