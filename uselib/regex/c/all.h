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


typedef struct Parser Parser;
typedef struct CharOpt CharOpt;
typedef struct CharResult CharResult;

// asm/chr.s
char change_byte_chr(volatile Parser *);
int is_byte_digit(char);

// asm/range.s
char* shorthand_class_ranges(char);
