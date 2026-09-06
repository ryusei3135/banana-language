#ifndef ALLOCATOR_H
#define ALLOCATOR_H

#include <stddef.h>

typedef unsigned long usize;

int allocator_init(size_t size);
void *alloc(size_t size);
void *mem_malloc(size_t size);
void mem_free(void *ptr);
void allocator_destroy(void);

#endif