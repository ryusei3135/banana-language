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
    if (__r.kind == Err)\
        return __r;\
    push_node(nodes, nodes->nodes[__r.ok]);

    
#define RetRangeNode(s, e) {\
    bump(self);\
    long __range = make_range_pair(nodes, s, e);\
    long __idx = make_node(nodes, Repeat, atom.ok, __range);\
    return make_ok_result(__idx); \
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


Parser parse_new(const char* pattern) {
    static Parser parse;
    int len = simd_strcpy(parse.chars, pattern);
    parse.chars_len = len;
    parse.pos = 0;
    parse.group_count = 0;
    return parse;
}


NodeResult parse_alt(Parser *self, Nodes *nodes) {
    long start = nodes->len;

    /* 修正: 最初の枝も他の枝と同様に nodes に積む（元コードは積んでいなかった） */
    PushNode(parse_concat(self, nodes));

    while (match_chr(self, '|')) {
        bump(self);
        PushNode(parse_concat(self, nodes));
    }

    /* 修正: グローバルな nodes->len ではなく、この parse_alt 呼び出しで
       追加された枝の数（差分）で「分岐が1つだけか」を判定する */
    if (nodes->len - start == 1) {
        return make_ok_result(pop_node(nodes));
    } else {
        long idx = make_range_pair(nodes, start, nodes->len);
        long alt_idx = make_alt_node(nodes, idx);
        return make_ok_result(alt_idx);
    }
}

static NodeResult parse_concat(Parser *self, Nodes *nodes) {
    long nodes_start = nodes->len;
    while (peek(self).kind == Some) {
        char c = *peek(self).value; /* 修正: 逆参照忘れ (*) を追加 */
        if (c == '|' || c == ')')
            break;
        /* 修正: parse_concat の自己再帰(無限ループ)になっていたのを
           parse_repeat の呼び出しに修正 */
        PushNode(parse_repeat(self, nodes));
    }
    long idx = make_range_pair(nodes, nodes_start, nodes->len);
    /* 修正: ここは Alt ではなく Concat を作る箇所。
       parse_bound の '{' リテラル化フォールバックと同じ
       「Range 種別で複数ノードの並びをラップする」パターンに合わせた。★要 all.h 確認 */
    long concat_idx = make_node(nodes, Range, idx, 0);
    return make_ok_result(concat_idx);
}


static NodeResult parse_repeat(Parser *self, Nodes *nodes) {
    NodeResult atom = parse_atom(self, nodes);
    if (atom.kind == Err)
        return atom;

    /* 修正: 誤って付いていたセミコロンを削除。
       これがあると if の中身が空文になり、量指定子(*+?{})が
       一切適用されなくなる致命的バグだった */
    if (peek(self).kind == None)
        return make_ok_result(atom.ok);

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

static NodeResult parse_bound(Parser *self, Nodes *nodes, long atom) {
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
        // "{" が数量子として不正 -> リテラルの '{' として扱う
        self->pos = checkpoint + 1;
        long right = make_node(nodes, Char, (long)'{', 0);
        long range_idx = make_range_pair(nodes, atom, right);
        long idx = make_node(nodes, Range, range_idx, 0);
        return make_ok_result(idx);
    }

    char* max[2] = {(char*)0xFF, (char*)0xFF};
    struct { char* start; char* end; } max_s = {0, 0};

    if (match_chr(self, ',')) {
        max_s.start = peek(self).value;
        bump(self);
        /* 修正: カンマ演算子になっていた条件を正しい比較に修正
           (元は while (peek(self).kind, Some) で常に true 扱いだった) */
        while (peek(self).kind == Some) {
            char *c = peek(self).value;
            if (is_byte_digit(*c) == 1) {
                max_s.end = c;
                bump(self);
            } else {
                break;
            }
        }
        if (max_s.end != 0) {
            max[0] = max_s.start;
            max[1] = max_s.end;
        }
    } else {
        /* 修正: ',' が無い場合は Rust版で `Some(min)` となる箇所、
           つまり max 側を min の値で埋めるべきところを、元コードは
           逆に min 側を未初期化の max_s(0,0) で潰してしまっていた */
        max[0] = min_s[0];
        max[1] = min_s[1];
    }

    if (unmatch_bump(self, '}'))
        return make_err_result("'{' に対応する '}' がありません");

    long idx = make_range_pair(
        nodes,
        parse_num(min_s[0], min_s[1]),
        parse_num(max[0], max[1])
    );
    return make_ok_result(make_node(nodes, Repeat, idx, 0));
}


static NodeResult parse_atom(Parser *self, Nodes *nodes) {
    CharOpt c0 = bump(self);
    /* 修正: カンマ演算子になっていた条件を正しい比較に修正
       (元は if (c0.kind, None) で常に false 扱いだった) */
    if (c0.kind == None)
        return make_err_result("パターンが予期せず終了しました");

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
        /* 修正: parse_alt のエラーを伝播せずに unmatch_bump へ進んでいた
           (Rust版の `?` に相当する処理が抜けていた) */
        if (inner.kind == Err)
            return inner;
        if (unmatch_bump(self, ')'))
            return make_err_result("'(' に対応する ')' がありません");
        if (group_idx != -1) {
            /* 修正: 未定義変数 i の使用、inner.value(存在しないフィールド)、
               Group 種別引数の欠落を修正 */
            long node_idx = make_node(nodes, Group, inner.ok, group_idx);
            return make_ok_result(node_idx);
        } else {
            return inner;
        }
    }
    case '.': {
        long idx = make_one_node(nodes, Any);
        return make_ok_result(idx);
    }
    case '^': {
        long idx = make_one_node(nodes, Start);
        return make_ok_result(idx);
    }
    case '$': {
        long idx = make_one_node(nodes, End);
        return make_ok_result(idx);
    }
    case '[':
        return parse_class(self, nodes);
    case '\\':
        /* 修正: 末尾がカンマになっており構文エラーだった箇所をセミコロンに修正 */
        return parse_escape(self, nodes);
    default: {
        /* 修正: 未定義変数 c を使用していた箇所を *c0.value に修正 */
        long idx = make_node(nodes, Char, (long)*c0.value, 0);
        return make_ok_result(idx);
    }
    }
}

/* ★ 以下 3 つは all.h に実体が無い前提のヘルパー。
   [0-9] / [a-zA-Z0-9_] / [ \t\n\r] という Rust版と同一のレンジ集合を
   アリーナに積んで Class ノードを作るためのものです。
   実際の range 追加 API / Class ノード生成 API 名に置き換えてください。 */
static long make_class_from_pairs(Nodes *nodes, const char pairs[][2], int count, int negated) {
    long start = nodes->len; /* ★ 要 all.h 確認: range 用バッファの長さフィールド名 */
    for (int i = 0; i < count; i++) {
        make_range_pair(nodes, pairs[i][0], pairs[i][1]); /* ★ 要 all.h 確認 */
    }
    return make_node(nodes, start, nodes->len, negated); /* ★ 要 all.h 確認 */
}

static NodeResult parse_escape(Parser *self, Nodes *nodes) {
    // エスケープされる文字
    if (peek(self).kind == None)
        return make_err_result("末尾がバックスラッシュで終わっています");

    char c = *peek(self).value;

    // エスケープ対象も消費する
    bump(self);

    switch (c) {
    case 'd': case 'D': {
        static const char pairs[][2] = {{'0','9'}};
        long idx = make_class_from_pairs(nodes, pairs, 1, c == 'D');
        return make_ok_result(idx);
    }
    case 'w': case 'W': {
        static const char pairs[][2] = {{'a','z'}, {'A','Z'}, {'0','9'}, {'_','_'}};
        long idx = make_class_from_pairs(nodes, pairs, 4, c == 'W');
        return make_ok_result(idx);
    }
    case 's': case 'S': {
        static const char pairs[][2] = {{' ',' '}, {'\t','\t'}, {'\n','\n'}, {'\r','\r'}};
        long idx = make_class_from_pairs(nodes, pairs, 4, c == 'S');
        return make_ok_result(idx);
    }
    case 'n':
        return make_ok_result(make_node(nodes, Char, (long)'\n', 0));
    case 't':
        return make_ok_result(make_node(nodes, Char, (long)'\t', 0));
    case 'r':
        return make_ok_result(make_node(nodes, Char, (long)'\r', 0));
    default:
        // \. \* \\ など、そのままリテラル化
        return make_ok_result(make_node(nodes, Char, (long)c, 0));
    }
}

static NodeResult parse_class(Parser *self, Nodes *nodes) {
    int negated = 0;
    if (match_chr(self, '^')) {
        negated = 1;
        bump(self);
    }

    long ranges_start = nodes->len; /* ★ 要 all.h 確認 */
    int first = 1;

    for (;;) {
        if (match_chr(self, ']') && !first) {
            bump(self);
            break;
        } else if (peek(self).kind == None) {
            return make_err_result("'[' に対応する ']' がありません");
        }
        first = 0;

        if (match_chr(self, '\\')
            && peek2(self).kind == Some
            && (*peek2(self).value == 'd' || *peek2(self).value == 'w' || *peek2(self).value == 's'))
        {
            bump(self); // '\\' を消費
            char kind = *bump(self).value; // d/w/s を消費
            switch (kind) {
            case 'd':
                make_range_pair(nodes, '0', '9'); /* ★ 要 all.h 確認 */
                break;
            case 'w':
                make_range_pair(nodes, 'a', 'z');
                make_range_pair(nodes, 'A', 'Z');
                make_range_pair(nodes, '0', '9');
                make_range_pair(nodes, '_', '_');
                break;
            case 's':
                make_range_pair(nodes, ' ', ' ');
                make_range_pair(nodes, '\t', '\t');
                make_range_pair(nodes, '\n', '\n');
                make_range_pair(nodes, '\r', '\r');
                break;
            }
            continue;
        }

        /* 修正: 元コード(Rust貼り付け)は .unwrap() でエラーを握りつぶし
           panic していた。他の箇所と同様に RegexError として伝播させる */
        CharResult r1 = parse_class_char(self);
        if (r1.kind == Err) {
            NodeResult err = { .err = r1.err, Err };
            return err;
        }
        char c1 = r1.ok;

        if (match_chr(self, '-')
            && peek2(self).kind == Some
            && *peek2(self).value != ']')
        {
            bump(self); // '-'
            CharResult r2 = parse_class_char(self);
            if (r2.kind == Err) {
                NodeResult err = { .err = r2.err, Err };
                return err;
            }
            make_range_pair(nodes, c1, r2.ok); /* ★ 要 all.h 確認 */
        } else {
            make_range_pair(nodes, c1, c1);
        }
    }

    long idx = make_node(nodes, ranges_start, nodes->len, negated); /* ★ 要 all.h 確認 */
    return make_ok_result(idx);
}