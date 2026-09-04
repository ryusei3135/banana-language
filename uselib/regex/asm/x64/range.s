
.data
.align 4                     # 4バイトアライメントに揃える
empty:
    .byte 255, 255, 255
.align 4
num:
    .byte 1
    .byte '0', '9'
.align 4
alpha:
    .byte 4
    .byte 'a', 'z'
    .byte 'A', 'Z'
    .byte '0', '9'
    .byte '_', '_'
.align 4
space:
    .byte 4
    .byte ' ', ' '
    .byte '\t', '\t'
    .byte '\n', '\n'
    .byte '\r', '\r'

.text
.global shorthand_class_ranges
.type shorthand_class_ranges, @function

shorthand_class_ranges:
    pushq %rbp
    movq %rsp, %rbp
    subq $16, %rsp

    movzbl %dil, %ecx
    cmpl $'d', %ecx
    jnz .N0
    leaq num(%rip), %rax
    jmp .return
.N0:
    cmpl $'w', %ecx
    jnz .N1
    leaq alpha(%rip), %rax
    jmp .return
.N1:
    cmpl $'s', %ecx
    jnz .N2
    leaq space(%rip), %rax
    jmp .return
.N2:
    leaq empty(%rip), %rax
.return:
    leave
    ret