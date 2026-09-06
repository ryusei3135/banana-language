#include "../mem.h"

/* mem.s (Linux) が提供する生のシステムコールラッパー */
extern void *sys_mmap(void *addr, unsigned long length, int prot,
                       int flags, int fd, unsigned long offset);
extern long sys_munmap(void *addr, unsigned long length);

#define PROT_READ   0x1
#define PROT_WRITE  0x2
#define MAP_PRIVATE 0x02
#define MAP_ANON    0x20

/* mmap は失敗時 (void*)-1 (= MAP_FAILED) を返す */
#define MAP_FAILED ((void *)-1)

/*
 * allocator.c から呼ばれる OS 依存のページ確保/解放。
 * Linux では anonymous private mmap を使い、libc には一切依存しない。
 */
void *os_alloc_pages(usize size)
{
    void *p = sys_mmap(
        0,                         /* addr   : カーネル任せ         */
        size,                      /* length                        */
        PROT_READ | PROT_WRITE,    /* prot                          */
        MAP_PRIVATE | MAP_ANON,    /* flags                         */
        -1,                        /* fd     : anonymous なので不要 */
        0                          /* offset                        */
    );
    if (p == MAP_FAILED)
        return 0;
    return p;
}

int os_free_pages(void *ptr, usize size)
{
    if (ptr == 0)
        return -1;
    long r = sys_munmap(ptr, size);
    return (r == 0) ? 0 : -1;
}
