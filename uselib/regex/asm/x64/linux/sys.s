
.global sys_exit
.type sys_exit, @function

sys_exit:
    movq $60, %rax
    movq $1, %rdi
    syscall