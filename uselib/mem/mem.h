#ifndef OS_MEMORY_H
#define OS_MEMORY_H

typedef unsigned long usize;

void *os_alloc_pages(usize size);
int os_free_pages(void *ptr, usize size);

#endif