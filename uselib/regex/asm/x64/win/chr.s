```asm
.text

.global change_byte_chr
.global is_byte_digit
.global simd_strcpy
.global parse_num
.global get_strlen

.type change_byte_chr, @function
.type is_byte_digit, @function
.type simd_strcpy, @function
.type parse_num, @function
.type get_strlen, @function

.extern peek
.type peek, @function


change_byte_chr:
    pushq %rbp
    movq %rsp, %rbp

    # Windows x64:
    # 呼び出し先のための shadow space 32 byte
    # + 自分の保存領域
    subq $48, %rsp

    # self を保存
    movq %rcx, 32(%rsp)

    # peek(self)
    call peek

    # self を復元
    movq 32(%rsp), %rcx

    # peek の戻り値の value を想定
    # AL = char
    movzbl %al, %eax

    # '\n'
    cmpl $110, %eax
    jnz .L_change_1

    movl $10, %eax
    leave
    ret

.L_change_1:
    # '\t'
    cmpl $116, %eax
    jnz .L_change_2

    movl $9, %eax
    leave
    ret

.L_change_2:
    # '\r'
    cmpl $114, %eax
    jnz .L_change_3

    movl $13, %eax
    leave
    ret

.L_change_3:
    # そのまま返す
    leave
    ret


is_byte_digit:
    pushq %rbp
    movq %rsp, %rbp

    movzbl %cl, %eax

    # 48以上
    cmpl $48, %eax
    jl .L_digit_err

    # 57以下
    cmpl $57, %eax
    jg .L_digit_err

    movl $1, %eax

    popq %rbp
    ret

.L_digit_err:
    xorl %eax, %eax

    popq %rbp
    ret

get_strlen:
    pushq %rbp
    movq %rsp, %rbp

    xorl %eax, %eax

.L_strlen_loop:
    movb (%rcx), %dl

    testb %dl, %dl
    jz .L_strlen_done

    incl %eax
    incq %rcx

    jmp .L_strlen_loop

.L_strlen_done:
    popq %rbp
    ret


simd_strcpy:
    pushq %rbp
    movq %rsp, %rbp

    # 元のdstを保存
    pushq %rdx

    # 元コードではYMMレジスタを使用
    vpxor %ymm1, %ymm1, %ymm1


.L_simd_loop:
    # -------------------------------------------------------
    # src のページ末尾付近なら scalar に切り替える
    # --------------------------------------------------------
    movq %rcx, %rax
    andq $0xfff, %rax
    cmpq $0xfe0, %rax
    ja .L_scalar
    # dst のページ末尾付近
    movq %rdx, %rax
    andq $0xfff, %rax
    cmpq $0xfe0, %rax
    ja .L_scalar
    # 32 byte load
    vmovdqu (%rcx), %ymm0
    # NULL byte検索
    vpcmpeqb %ymm1, %ymm0, %ymm2
    # 比較結果をbit maskへ
    vpmovmskb %ymm2, %eax
    testl %eax, %eax
    jnz .L_found_null
    # NULLがなければ32 byteコピー
    vmovdqu %ymm0, (%rdx)
    addq $32, %rcx
    addq $32, %rdx
    jmp .L_simd_loop


# ------------------------------------------------------------
# scalar copy
# ------------------------------------------------------------

.L_scalar:
    movb (%rcx), %al
    movb %al, (%rdx)
    incq %rcx
    incq %rdx
    testb %al, %al
    jnz .L_simd_loop
    jmp .L_simd_done

# ------------------------------------------------------------
# NULL発見
# ------------------------------------------------------------

.L_found_null:

    # 最初のNULLの位置
    #
    # EAX:
    #   00010000...
    #
    # TZCNT:
    #   最下位bitまでのbit数
    #
    # = NULLまでのbyte数
    tzcntl %eax, %ecx
.L_copy_remaining:
    movb (%rcx), %al
    movb %al, (%rdx)
    incq %rcx
    incq %rdx
    decl %ecx
    jns .L_copy_remaining
.L_simd_done:
    # 元のdst
    movq (%rsp), %rax
    # コピー後dst - 元dst
    subq %rax, %rdx
    # NULL分を除く
    decq %rdx
    movq %rdx, %rax
    vzeroupper
    popq %rdx
    popq %rbp
    ret


# ============================================================
# int parse_num(const char* start, const char* end)
#
# Windows x64 ABI
#
#   RCX = start
#   RDX = end
#
# 戻り値:
#   EAX = 整数
#
# 例:
#   parse_num("123", "123"+3)
#       -> 123
#
#   parse_num("-123", "-123"+4)
#       -> -123
# ============================================================

parse_num:
    pushq %rbp
    movq %rsp, %rbp

    # result = 0
    xorl %eax, %eax

    # negative = 0
    xorl %r8d, %r8d


    # start >= end ?
    cmpq %rdx, %rcx
    jae .L_parse_done


    # 最初の文字
    movzbl (%rcx), %r9d

    # '-'
    cmpl $45, %r9d
    jne .L_loop_digits

    # negative = 1
    movl $1, %r8d

    incq %rcx


.L_loop_digits:
    # start >= end
    cmpq %rdx, %rcx
    jae .L_apply_sign
    # 現在の文字
    movzbl (%rcx), %r9d
    # '0'～'9' の判定
    subl $48, %r9d
    cmpl $9, %r9d
    ja .L_apply_sign
    # result = result * 10 + digit
    imull $10, %eax
    addl %r9d, %eax
    incq %rcx
    jmp .L_loop_digits


.L_apply_sign:
    testl %r8d, %r8d
    jz .L_parse_done
    negl %eax


.L_parse_done:
    popq %rbp
    ret
```
