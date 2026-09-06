#pragma once


struct Parser {
    char* chars;
    long chars_len;
    long pos;
    long group_count;
};


typedef enum OpKind {
    None,
    Some,
} OpKind;

typedef enum ResultKind {
    Ok,
    Err,
} ResultKind;

typedef enum OpKind OpKind;
typedef enum ResultKind ResultKind;

struct CharOpt
{
    char* value;
    OpKind kind;
};

struct CharResult {
    union {
        char ok;
        char err[256];
    };
    ResultKind kind;
};


typedef struct NodeResult {
    union {
        long ok;
        char err[256];
    };
    ResultKind kind;
} NodeResult;


typedef enum NodeKind {
    Char,
    Any,
    Start,
    End,
    Class,  // left = レンジ一覧(Rangeノード)のindex, right = 否定フラグ
    Concat, // left = 子ノード列(Rangeノード)のindex
    Alt,    // left = 子ノード列(Rangeノード)のindex
    Repeat, // left = 中身のindex, right = (min,max)を持つRangeノードのindex
    Group,  // left = 中身のindex, right = キャプチャ番号 (1始まり)

    Range, // l = start, r = end
    NodesK, // is ptr
    Flag,  // r = bool
} NodeKind;

typedef struct Node {
    NodeKind kind;
    long left;
    long right;
} Node;


#define MaxLen 2048

typedef struct Nodes {
    Node nodes[MaxLen];
    Node *pos;
    long len;
    long max_len;
} Nodes;


typedef struct Parser Parser;
typedef struct CharOpt CharOpt;
typedef struct CharResult CharResult;

CharOpt peek(Parser *this);

// c/parser.c
Parser parse_new(const char* pattern, long len);
NodeResult parse_alt(Parser *self, Nodes *nodes);
void parser_drop(Parser *self);


// asm/chr.s
char change_byte_chr(volatile Parser *);
int is_byte_digit(char);
int simd_strcpy(char*, const char*);
int parse_num(const char* start, const char* end);

// asm/range.s
char* shorthand_class_ranges(char);


// c/result.c
NodeResult ok_val(long len);
NodeResult make_err_result(char *msg);

// c/node.c
/* 修正: 元は `Nodes ini_nodes();` で構造体を値渡し(約49KB)で返していた。
   Rust 側 (src/regex.rs) は `fn ini_nodes() -> &'static mut Nodes;` と
   ポインタ返却を前提に宣言しているため、SysV ABI 的には x86-64 では
   16byte超の構造体返却は「呼び出し側が隠しポインタ引数(RDI)を渡し、
   関数がそこへ書き込む」規約になる。Rust はポインタ返却だと思って
   その隠しポインタを渡さないため、C側は不定値の RDI へ 49KB を
   memcpy してしまい、確実にクラッシュする。ポインタを返す形に修正。 */
Nodes* ini_nodes();
void push_node(Nodes *nodes, Node new_node);
long pop_node(Nodes *nodes);
long make_range_pair(Nodes *nodes, long start, long end);
long make_alt_node(Nodes *nodes, long range_idx);
long make_concat_node(Nodes *nodes, long range_idx);
long make_node(Nodes *nodes, NodeKind kind, long left, long right);
long make_one_node(Nodes *nodes, NodeKind kind);
