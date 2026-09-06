.text

.global os_alloc_pages
.global os_free_pages


.extern VirtualAlloc
.extern VirtualFree


# void *os_alloc_pages(usize size)

os_alloc_pages:

    subq $40, %rsp

    # lpAddress = NULL
    xorq %rcx, %rcx

    # dwSize
    movq %rdi, %rdx

    # MEM_COMMIT | MEM_RESERVE
    movl $0x3000, %r8d

    # PAGE_READWRITE
    movl $0x04, %r9d

    call VirtualAlloc

    addq $40, %rsp

    ret


# int os_free_pages(void *ptr, usize size)

os_free_pages:

    subq $40, %rsp

    # lpAddress
    movq %rdi, %rcx

    # dwSize = 0
    xorq %rdx, %rdx

    # MEM_RELEASE
    movl $0x8000, %r8d

    call VirtualFree

    addq $40, %rsp

    ret