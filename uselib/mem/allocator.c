#include "allocator.h"
#include "mem.h"

#define ALIGNMENT 16

typedef struct Block
{
    usize size;
    struct Block *next;
    int free;
} Block;

static void *heap_start = 0;
static usize heap_size = 0;
static Block *first_block = 0;

/* 自動初期化に使うデフォルトヒープサイズ (4MiB) */
#define DEFAULT_HEAP_SIZE (4UL * 1024UL * 1024UL)

/* ================================= */
/* alignment                        */
/* ================================= */
static usize align16(usize size)
{
    return (size + 15UL) & ~15UL;
}

/* ================================= */
/* pointer conversion                */
/* ================================= */
static void *block_to_ptr(Block *block)
{
    return (void *)(
        (unsigned char *)block +
        sizeof(Block)
    );
}

static Block *ptr_to_block(void *ptr)
{
    return (Block *)(
        (unsigned char *)ptr -
        sizeof(Block)
    );
}

/* ================================= */
/* initialization                    */
/* ================================= */
int allocator_init(usize size)
{
    if (heap_start != 0
        || size < sizeof(Block) + ALIGNMENT)
        return -1;
    size = align16(size);
    void *memory =
        os_alloc_pages(size);
    if (memory == 0)
        return -1;
    heap_start = memory;
    heap_size = size;
    first_block =
        (Block *)memory;
    first_block->size =
        size - sizeof(Block);
    first_block->next = 0;
    first_block->free = 1;
    return 0;
}

/* ================================= */
/* split                            */
/* ================================= */
static void split_block(
    Block *block,
    usize size
)
{
    if (block->size <
        size + sizeof(Block) + ALIGNMENT)
        return;
    unsigned char *address =
        (unsigned char *)block +
        sizeof(Block) +
        size;
    Block *new_block =
        (Block *)address;
    new_block->size =
        block->size -
        size -
        sizeof(Block);
    new_block->next =
        block->next;
    new_block->free = 1;
    block->size = size;

    block->next =
        new_block;
}

/* ================================= */
/* alloc                            */
/* ================================= */
void *alloc(usize size)
{
    /*
     * 修正: allocator_init() を呼び忘れると first_block == 0 のままで
     * alloc() が常に NULL を返し、呼び出し元 (parser.c の mem_malloc 経由)
     * では元のセグフォと同じ「NULL に書き込む」事故が再発してしまう。
     * Rust 側からは allocator_init を呼ぶ手段がそもそも公開されていない
     * ため、未初期化なら初回利用時にデフォルトサイズで自動初期化する。
     */
    if (first_block == 0) {
        if (allocator_init(DEFAULT_HEAP_SIZE) != 0)
            return 0;
    }
    if (size == 0)
        return 0;
    size = align16(size);
    Block *block =
        first_block;
    while (block != 0)
    {
        if (block->free &&
            block->size >= size)
        {
            split_block(
                block,
                size
            );
            block->free = 0;
            return block_to_ptr(block);
        }
        block =
            block->next;
    }
    return 0;
}

/* ================================= */
/* malloc互換ラッパー                 */
/* ================================= */
/*
 * 修正: 元は関数名が libc の malloc/free と全く同じだった。
 * これを静的ライブラリとしてリンクすると、Rust 標準ライブラリが内部で
 * 使う malloc/free (Vec/String/Box 等あらゆるヒープ確保) までこの
 * 自前アロケータに奪われてしまう。ところが allocator_init() を呼ぶ前は
 * heap_start == 0 のままなので alloc() は常に NULL を返し、
 * Rust 側は「メモリ確保に失敗した」として即 abort していた。
 * (エラーメッセージ "memory allocation of N bytes failed" は
 *  Rust 標準ライブラリの OOM ハンドラそのもの)
 *
 * libc の malloc/free を上書きしてしまわないよう、名前を
 * mem_malloc/mem_free に変更する。
 */
void *mem_malloc(usize size)
{
    return alloc(size);
}

/* ================================= */
/* merge                            */
/* ================================= */
static void merge_blocks(void)
{
    Block *block =
        first_block;
    while (block != 0 &&
           block->next != 0)
    {
        Block *next =
            block->next;
        if (block->free &&
            next->free)
        {
            block->size +=
                sizeof(Block) +
                next->size;
            block->next =
                next->next;
            continue;
        }
        block =
            block->next;
    }
}

/* ================================= */
/* free                             */
/* ================================= */
void mem_free(void *ptr)
{
    if (ptr == 0)
        return;
    unsigned char *p =
        (unsigned char *)ptr;

    unsigned char *start =
        (unsigned char *)heap_start;

    unsigned char *end =
        start + heap_size;
    /*
        ヒープ外のアドレスを
        freeしない
    */
    if (p < start || p >= end)
        return;
    Block *block =
        ptr_to_block(ptr);
    block->free = 1;
    merge_blocks();
}

/* ================================= */
/* destroy                          */
/* ================================= */
void allocator_destroy(void)
{
    if (heap_start == 0)
        return;
    os_free_pages(
        heap_start,
        heap_size
    );
    heap_start = 0;
    heap_size = 0;
    first_block = 0;
}