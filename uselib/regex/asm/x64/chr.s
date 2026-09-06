.text
.global change_byte_chr
.type change_byte_chr, @function

.global is_byte_digit
.type is_byte_digit, @function

.global simd_strcpy
.type simd_strcpy, @function

.global parse_num
.type parse_num, @function

.extern peek
.type peek, @function

change_byte_chr:
    pushq %rbp
    movq %rsp, %rbp
    subq $16, %rsp

    movq %rdi, (%rsp)

    call peek

    
    movzbq %al, %rcx           # %raxの下位1バイト(value)を %rcx にゼロ拡張でコピー
    
    # \\n
    cmp $110, %rcx
    jnz .N0
    mov $10, %rax           # \\n (10) を戻り値にする
    leave
    ret
    
.N0: # \\t
    cmp $116, %rcx
    jnz .N1
    mov $9, %rax            # \\t (9) を戻り値にする
    leave
    ret
    
.N1: # \\r
    cmp $114, %rcx
    jnz .N2
    mov $13, %rax           # \\r (13) を戻り値にする
    leave
    ret
    
.N2:
    mov %rcx, %rax          # マッチしなかった場合は、読み込んだ文字コードをそのまま返す
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


simd_strcpy:
    pushq %rbp
    movq %rsp, %rbp
    pushq %rdi
    vpxor   %ymm1, %ymm1, %ymm1 # %ymm1 をすべて 0 に初期化 (\0 検出用)
.L_loop:
    vmovdqu (%rsi), %ymm0       # src から 32 バイト読み込み
    vpcmpeqb %ymm1, %ymm0, %ymm2 # ymm0 の各バイトが \0 か比較。一致なら 0xFF、不一致なら 0x00
    vpmovmskb %ymm2, %eax        # 比較結果を 32 ビットのビットマスクに変換

    testl %eax, %eax          # \0 が見つかったかチェック
    jnz .L_found_null       # 0 でなければ（\0 があれば）ループ脱出へ

    # \0 が見つからない場合は 32 バイトをそのままコピーして次へ
    vmovdqu %ymm0, (%rdi)
    addq $32, %rsi
    addq $32, %rdi
    jmp .L_loop

.L_found_null:
    # ビットマスク（%eax）の最下位ビットから連続する 0 の個数を数える (tzcnt)
    # これにより、32バイト中の何番目に \0 があったかがわかる
    tzcntl %eax, %ecx           # %ecx = \0 までのバイト数

    # 残りのバイト（\0 を含む）を1バイトずつコピー
.L_copy_remaining:
    movb (%rsi), %dl
    movb %dl, (%rdi)
    incq %rsi
    incq %rdi
    decl %ecx
    jns .L_copy_remaining

    movq (%rsp), %rax
    subq %rax, %rdi
    decq %rdi
    movq %rdi, %rax

    popq %rdi
    popq %rbp
    ret


# int parse_num(const char* start, const char* end);
# 引数: %rdi = start, %rsi = end
# 戻り値: %rax = 変換された整数
parse_num:
    pushq %rbp
    movq %rsp, %rbp

    xorq %rax, %rax
    xorl %ecx, %ecx

    cmpq %rsi, %rdi          # start == end の場合は即終了
    jae .L_done

    # 最初の文字が '-'（マイナス）かチェック
    movzbq  (%rdi), %rdx
    cmpb $45, %dl            # '-' の ASCII コードは 45
    bne .L_loop_digits
    movl $1, %ecx
    incq %rdi

.L_loop_digits:
    cmpq %rsi, %rdi
    jae .L_apply_sign

    movzbq (%rdi), %rdx
    
    # '0' (48) 〜 '9' (57) の範囲チェック
    subq $48, %rdx
    cmpq $9, %rdx
    ja .L_apply_sign

    # %rax = %rax * 10 + %rdx の計算
    imulq $10, %rax
    addq %rdx, %rax

    incq %rdi
    jmp .L_loop_digits
.L_apply_sign:
    testl %ecx, %ecx
    jz .L_done
    negq %rax  

.L_done:
    popq %rbp
    ret
