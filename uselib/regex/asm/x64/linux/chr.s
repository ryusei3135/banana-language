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
    subq $16, %rsp

    movq %rdi, (%rsp)
    call peek
    movq (%rsp), %rdi
    
    movzbl %al, %ecx           # %eaxの下位1バイト(value)を %ecx にゼロ拡張でコピー
    # \\n
    cmpl $110, %ecx
    jnz .N0
    movl $10, %eax           # \\n (10) を戻り値にする
    leave
    ret
.N0: # \\t
    cmpl $116, %ecx
    jnz .N1
    movl $9, %eax            # \\t (9) を戻り値にする
    leave
    ret
.N1: # \\r
    cmpl $114, %ecx
    jnz .N2
    movl $13, %eax           # \\r (13) を戻り値にする
    leave
    ret
.N2:
    mov %ecx, %eax          # マッチしなかった場合は、読み込んだ文字コードをそのまま返す
    leave
    ret


is_byte_digit:
    pushq %rbp
    movq %rsp, %rbp
    subq $16, %rsp

    movzbl  %dil, %eax
    # 48以上57以下の場合のみ1を返す
    cmpl $48, %eax
    jl .Err
    cmpl $57, %eax
    jg .Err
    movl $1, %eax
    leave
    ret
.Err:
    movl $0, %eax
    leave
    ret


// int (const char* msg)
get_strlen:
    pushq %rbp
    movq %rsp, %rbp
    xorl %eax, %eax
    movq %rdi, (%rsp)
.L0:
    movb (%rdi), %cl
    cmpb $0, %cl
    jz .E0
    incl %eax
    incq %rdi
    jmp .L0
.E0:
    movq (%rsp), %rdi
    leave
    ret

simd_strcpy:
    pushq %rbp
    movq %rsp, %rbp
    pushq %rdi

    # ymm1 = 0
    vpxor %ymm1, %ymm1, %ymm1
.L_loop:
    movq %rsi, %rax
    andq $0xfff, %rax
    cmpq $0xfe0, %rax
    ja .L_scalar
    movq %rdi, %rax
    andq $0xfff, %rax
    cmpq $0xfe0, %rax
    ja .L_scalar
    vmovdqu (%rsi), %ymm0

    # NULL byteを探す
    vpcmpeqb %ymm1, %ymm0, %ymm2

    # 各byteの比較結果をbit maskへ
    vpmovmskb %ymm2, %eax

    testl %eax, %eax
    jnz .L_found_null

    # NULLがなければ32byteコピー
    vmovdqu %ymm0, (%rdi)

    addq $32, %rsi
    addq $32, %rdi

    jmp .L_loop
.L_scalar:
    movb (%rsi), %al
    movb %al, (%rdi)

    incq %rsi
    incq %rdi

    testb %al, %al
    jnz .L_loop

    jmp .L_done
.L_found_null:
    # 最初のNULLの位置
    #
    # eax:
    #   00010000...
    #
    # tzcnt:
    #   最下位の1までのbit数
    #
    # = NULLまでのbyte数
    #
    tzcntl %eax, %ecx
.L_copy_remaining:
    movb (%rsi), %dl
    movb %dl, (%rdi)
    incq %rsi
    incq %rdi
    decl %ecx
    jns .L_copy_remaining
.L_done:
    movq (%rsp), %rax
    subq %rax, %rdi
    decq %rdi
    movq %rdi, %rax
    vzeroupper
    popq %rdi
    popq %rbp
    ret


# int parse_num(const char* start, const char* end);
# 引数: %rdi = start, %rsi = end
# 戻り値: %rax = 変換された整数
parse_num:
    pushq %rbp
    movq %rsp, %rbp

    xorl %eax, %eax
    xorl %ecx, %ecx

    cmpq %rsi, %rdi          # start == end の場合は即終了
    jae .L_done1
    # 最初の文字が '-'（マイナス）かチェック
    movzbl (%rdi), %edx
    cmpb $45, %dl            # '-' の ASCII コードは 45
    jne .L_loop_digits
    movl $1, %ecx
    incq %rdi
.L_loop_digits:
    cmpq %rsi, %rdi
    jae .L_apply_sign

    movzbl (%rdi), %edx
    # '0' (48) 〜 '9' (57) の範囲チェック
    subl $48, %edx
    cmpl $9, %edx
    ja .L_apply_sign
    # %rax = %rax * 10 + %rdx の計算
    imull $10, %eax
    addl %edx, %eax

    incq %rdi
    jmp .L_loop_digits
.L_apply_sign:
    testl %ecx, %ecx
    jz .L_done1
    negl %eax  
.L_done1:
    popq %rbp
    ret
