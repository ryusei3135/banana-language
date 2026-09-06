#pragma once


struct Parser {
    char* chars;
    long chars_len;
    long pos;
    long group_count;
};


enum OpKind {
    None,
    Some,
};

enum ResultKind {
    Ok,
    Err,
};

typedef enum OpKind OpKind;
typedef enum ResultKind ResultKind;

struct CharOpt
{
    char value;
    OpKind kind;
};

struct CharResult {
    union {
        char ok;
        char* err;
    };
    ResultKind kind;
};





typedef enum ResultKind {
    Ok,
    Err,
} ResultKind;


typedef union NodeVal {
    long ok;
    char* err;
} NodeVal;


typedef struct NodeResult {
    NodeVal v;
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

typedef struct Nodes {
    Node nodes[2048];
    Node *pos;
    long len;
} Nodes;


typedef struct Parser Parser;
typedef struct CharOpt CharOpt;
typedef struct CharResult CharResult;

// asm/chr.s
char change_byte_chr(volatile Parser *);
int is_byte_digit(char);
int simd_strcpy(char*, const char*);
int parse_num(const char* start, const char* end);

// asm/range.s
char* shorthand_class_ranges(char);


// c/node.c
Nodes ini_nodes();
void push_node(Nodes *nodes, Node new_node);
long pop_node(Nodes *nodes);
long make_range_pair(Nodes *nodes, long start, long end);
long make_alt_node(Nodes *nodes, long range_idx);
long make_concat_node(Nodes *nodes, long range_idx);
long make_node(Nodes *nodes, NodeKind kind, long left, long right);
long make_one_node(Nodes *nodes, NodeKind kind);
