.text

.global sys_mmap
.global sys_munmap

.type sys_mmap, @function
.type sys_munmap, @function


# void *sys_mmap(
#     void *addr,
#     unsigned long length,
#     int prot,
#     int flags,
#     int fd,
#     unsigned long offset
# );

sys_mmap:
    # 修正: 元コードは r10/r8/r9 に addr/length/prot を先に退避したあと、
    # 「movq %r8,%r8」のような自己代入で fd/offset を復元したつもりに
    # なっていたが、実際には直前の movq で r8/r9 がそれぞれ length/prot に
    # 上書きされてしまっており、fd と offset に誤った値が渡っていた。
    # rdi(addr)/rsi(length)/rdx(prot)/r8(fd)/r9(offset) は
    # C の呼び出し規約とLinuxシステムコール規約でレジスタが一致しているので
    # そのまま渡せばよく、rcx(flags) だけ syscall 規約の r10 に積み直せばよい。
    movq %rcx, %r10        # flags
    movq $9, %rax          # SYS_mmap
    syscall
    ret


# int sys_munmap(void *addr, unsigned long length)

sys_munmap:
    movq $11, %rax         # SYS_munmap
    syscall
    ret