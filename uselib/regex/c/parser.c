#include "all.h"


CharOpt peek(Parser *this) {
    OpKind kind = Some;
    if (this->pos >= this->chars_len)
        kind = None;

    CharOpt opt = {
        kind == Some ? this->chars[this->pos] : 0,
        kind
    };
    return opt;
}

CharOpt peek2(Parser *this) {
    OpKind kind = Some;
    if (this->pos + 1 >= this->chars_len)
        kind = None;

    CharOpt opt = {
        kind == Some ? this->chars[this->pos + 1] : 0,
        kind
    };
    return opt;
}

CharOpt bump(Parser *this) {
    CharOpt c = peek(this);
    if (c.kind == Some) {
        this->pos += 1;
    }
    return c;
}

char match_chr(Parser *this, char chr) {
    if (peek(this).kind == None)
        return 0;
    if (peek(this).value == chr)
        return 1;
    return 0;
}

char match_chr_2(Parser *this, char chr) {
    if (peek2(this).kind == None)
        return 0;
    if (peek2(this).value == chr)
        return 1;
    return 0;
}

char unmatch_bump(Parser *this, char chr) {
    if (peek(this).kind == None)
        return 1;
    if (peek(this).value != chr)
        return 1;
    bump(this);
    return 0;
}


#define ResultErrGen(msg)\
    CharResult result = {.err = (msg), Err};\
    return result;

#define ResultOkGen(c)\
    CharResult result = {.ok = (c), Ok};\
    return result;

#define ResultOK(T, c)\
    T result = {.ok = c, Ok};\
    return result;

CharResult parse_class_char(Parser *this) {
    CharOpt c0 = peek(this);
    if (c0.kind == None) {
        ResultErrGen("'[' に対応する ']' がありません");
    }

    if (c0.value == '\\') {
        bump(this); // '\\' を消費
        if (peek(this).kind == None) {
            ResultErrGen("末尾がバックスラッシュで終わっています");
        }
        // change_byte_chr がエスケープされた文字自体の読み取り・消費を行う
        ResultOkGen(change_byte_chr(this));
    }

    bump(this); // 通常の文字を消費
    ResultOkGen(c0.value);
}

