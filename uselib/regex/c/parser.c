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
    /*
     * 修正: 以前は nodes_start をループの一番最初に一度だけ記録し、
     * 各要素ごとに PushNode マクロで「結果ノードを配列末尾へコピー」
     * していた。しかし PushNode はコピー元の(元々あった)ノードを
     * 消さずに残すため、[nodes_start, 最終len) の範囲には
     * 「各要素の元ノード」と「そのコピー」が両方含まれてしまい、
     * 例えば "abc" の連結が実際には6要素 (a,a,b,b,c,c) の列として
     * 組み立てられていた。ネストした構造 (グループ・文字クラス等) が
     * 混ざるとさらに、要素の"内部で使われているだけの補助ノード"まで
     * 一緒に取り込まれてしまう。
     *
     * ここでは各要素の parse_repeat() の戻り値 (index) だけを
     * いったん children[] に貯めておき、全要素を解析し終えてから
     * まとめて配列末尾にコピーする。こうすることで
     * [seq_start, nodes->len) には「各要素のコピーちょうど1つずつ」
     * だけが並ぶようになる。
     */
    long children[MaxLen];
    int child_count = 0;
    for (;;) {
        CharOpt r = peek(self);
        if (r.kind == None)
            break;
        if (*r.value == '|' || *r.value == ')')
            break;
        NodeResult res =
            parse_repeat(self, nodes);
        if (res.kind == Err)
            return res;
        if (child_count >= MaxLen)
            return make_err_result(
                "パターンが長すぎます\0"
            );
        children[child_count] = res.ok;
        child_count += 1;
    }
    long seq_start = nodes->len;
    for (int i = 0; i < child_count; i++)
        push_node(nodes, nodes->nodes[children[i]]);
    long idx =
        make_range_pair(
            nodes,
            seq_start,
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
    /*
     * 修正: parse_concat と同じ理由で、nodes_start をループの最初に
     * 一度だけ記録して [nodes_start, 最終len) をそのまま分岐リストと
     * みなす方式は誤り。各 '|' 区切りの parse_concat() 呼び出しは
     * 内部で多数のノード(その concat 自身の子要素やコピー)を
     * 作るため、その全てが分岐リストに紛れ込んでしまい、
     * 例えば "abc" 単体(分岐は1つだけのはず)が実際には
     * 「a, a, b, b, c, c, Range, Concat」のような
     * 文字単位のバラバラな分岐として扱われてしまっていた。
     *
     * parse_concat と同様、各分岐の parse_concat() の戻り値だけを
     * branches[] に貯めておき、全分岐を解析し終えてから
     * まとめて配列末尾にコピーする。
     *
     * また PushNode マクロは使わない(エラー時に一度解放したメッセージを
     * 上位でもう一度解放してしまう二重解放を避けるため。
     * parse_atom の '(' ケースで parse_alt の結果を素通しする書き方と
     * 同様に、エラーは解放せずそのまま返す)。
     */
    long branches[MaxLen];
    int branch_count = 0;

    NodeResult first =
        parse_concat(self, nodes);
    if (first.kind == Err)
        return first;
    branches[branch_count] = first.ok;
    branch_count += 1;

    while (match_chr(self, '|')) {
        bump(self);
        NodeResult next =
            parse_concat(self, nodes);
        if (next.kind == Err)
            return next;
        if (branch_count >= MaxLen)
            return make_err_result(
                "パターンが長すぎます\0"
            );
        branches[branch_count] = next.ok;
        branch_count += 1;
    }

    long seq_start = nodes->len;
    for (int i = 0; i < branch_count; i++)
        push_node(nodes, nodes->nodes[branches[i]]);
    long idx =
        make_range_pair(
            nodes,
            seq_start,
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
    /*
     * 修正: このパーサ内では複数の独立したバグが重なっていた。
     *
     * 1. parse_num(start, end) は asm 実装 (chr.s) を見ると
     *    [start, end) の半開区間 (end は「最後の数字の次」を指す)
     *    を前提にしている。ところが従来のコードは min_s[1] /
     *    max_s.end に「最後の数字そのもの」を指すポインタを
     *    格納していたため、1桁の数字 (例: "2") では start==end
     *    となり parse_num が即 0 を返し、2桁以上の数字では
     *    最後の1桁が読み飛ばされていた (例: "12" が "1" として
     *    読まれる)。
     *
     * 2. "{n,}" (上限なし) のケースで、上限が指定されなかった時の
     *    センチネルが (char*)0xFF 同士の組で、そのまま
     *    parse_num(0xFF, 0xFF) を呼ぶと start==end で 0 を返して
     *    しまい、本来 -1 (上限なし。match_node 側は
     *    `max_raw < 0` を「上限なし」として扱う) にすべきところが
     *    0 (0回の繰り返し=事実上マッチ不可) になっていた。
     *
     * 3. 出来上がった Repeat ノードを
     *    `make_node(nodes, Repeat, idx, 0)` として作っていたが、
     *    NodeKind::Repeat は「left=中身のindex, right=(min,max)の
     *    Rangeノードのindex」という規約 (lib.rs 側コメント参照) の
     *    はずが、left に (min,max) の Range を、right に
     *    ハードコードした 0 を入れてしまっていた
     *    (この 0 がたまたま atom の index と一致する時だけ
     *    それらしく見えていた)。
     *
     * これら3つが重なって、"{n}" / "{n,m}" / "{n,}" のいずれも
     * まともに機能していなかった。ここで一括して修正する。
     */
    long checkpoint = self->pos;
    bump(self); // '{' を消費済み

    char *min_start = self->chars + self->pos;
    while (
        peek(self).kind == Some
        &&
        is_byte_digit(*peek(self).value) == 1
    ) {
        bump(self);
    }
    char *min_end = self->chars + self->pos;

    if (min_end == min_start) {
        // '{' の直後に数字が無い -> 量指定子ではなくリテラル '{' として扱う
        self->pos =
            checkpoint + 1;
        long seq_start = nodes->len;
        push_node(nodes, nodes->nodes[atom]);
        make_node(
            nodes,
            Char,
            (long)'{',
            0
        );
        long range_idx =
            make_range_pair(
                nodes,
                seq_start,
                nodes->len
            );
        long idx =
            make_concat_node(nodes, range_idx);
        return ok_val(idx);
    }

    long min_val =
        (long)parse_num(min_start, min_end);
    long max_val;

    if (match_chr(self, ',')) {
        bump(self);
        char *max_start = self->chars + self->pos;
        while (
            peek(self).kind == Some
            &&
            is_byte_digit(*peek(self).value) == 1
        ) {
            bump(self);
        }
        char *max_end = self->chars + self->pos;
        if (max_end == max_start) {
            // "{n,}" -> 上限なし
            max_val = -1;
        } else {
            max_val = (long)parse_num(max_start, max_end);
        }
    } else {
        // "{n}" -> ちょうど n 回
        max_val = min_val;
    }

    if (unmatch_bump(self, '}'))
        return make_err_result(
            "'{' に対応する '}' がありません\0"
        );
    long idx =
        make_range_pair(
            nodes,
            min_val,
            max_val
        );
    return ok_val(
        make_node(
            nodes,
            Repeat,
            atom,
            idx
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
    char chr;
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
            chr = '\n';
            goto make_node;
        case 't':
            chr = '\t';
            goto make_node;
        case 'r':
            chr = '\r';
            goto make_node;
        default:
            chr = c;
    }
make_node:
    return ok_val(
        make_node(
            nodes,
            Char,
            (long)chr,
            0
        )
    );
}


/* ============================================================
 * Character class
 * ============================================================ */

static NodeResult parse_class(
    Parser *restrict self,
    Nodes *restrict nodes
) {
    /*
     * 修正: '[' は呼び出し元の parse_atom() が既に
     * `CharOpt c0 = bump(self);` で消費済み (c0.value=='[' で
     * ここへディスパッチしてきている)。にも関わらずここでも
     * unmatch_bump(self, '[') によってもう一度 '[' を読もうとしていたため、
     * 実際にはその次の文字 (例: 否定クラスの '^' や最初のクラス文字) を
     * '[' と比較して常に失敗し、"'[' がありません" エラーになっていた。
     * parse_escape など他の parse_atom ディスパッチ先と同様、
     * ここではディスパッチ文字を消費し直さない。
     */
    int negated = 0;
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
    char chr;
    switch (c) {
        case 'w': {
            chr = 'w';
            break;
        }
        case 's': {
            chr = 's';
            break;
        }
        default:
            return;
    }
    char (*range)[2] = shorthand_class_ranges(chr);
    for (int i=1; i < range[0][0];i++)
        make_range_pair(
            nodes, 
            range[i][0], 
            range[i][1]
        );
}
