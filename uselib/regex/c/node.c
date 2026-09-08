#include "all.h"




#ifdef _WIN32
__declspec(dllimport)
void ExitProcess(unsigned int);

void exit_me(void)
{
    ExitProcess(1);
}
#else

void sys_exit(void);

void exit_me(void) {
    sys_exit();
}
#endif

Nodes* ini_nodes() {
    /* 修正: 以前は関数内 static な単一バッファを全 Regex インスタンスで
       共有していたため、ある Regex を生かしたまま別の Regex::new を
       呼ぶと、後からコンパイルされたパターンが前のパターンの AST を
       上書きしてしまっていた (root インデックスは変わらないのに
       中身だけ差し替わる)。ヒープに毎回新しい領域を確保し、
       Regex ごとに独立したノード配列を持たせるように変更する。 */
    Nodes *n = (Nodes *)mem_malloc((long)sizeof(Nodes));
    n->pos = n->nodes;
    n->len = 0;
    n->max_len = MaxLen;
    return n;
}

void nodes_drop(Nodes *n) {
    mem_free(n);
}

void push_node(Nodes *nodes, Node new_node) {
    *nodes->pos = new_node;
    if (nodes->max_len <= nodes->len + 1)
        exit_me();
    nodes->pos++;
    nodes->len++;
}

long pop_node(Nodes *nodes) {
    nodes->pos--;
    return nodes->len - 1;
}


long make_range_pair(Nodes *nodes, long start, long end) {
    Node node = {
        .kind = Range,
        .left = start,
        .right = end,
    };
    push_node(nodes, node);
    return nodes->len-1;
}


#define MakeNode(K)\
    Node node = {\
        .kind = K,\
        .left = range_idx,\
        .right = 0,\
    };\
    push_node(nodes, node);\
    return nodes->len - 1;

long make_alt_node(Nodes *nodes, long range_idx) {
    MakeNode(Alt);
}

long make_concat_node(Nodes *nodes, long range_idx) {
    MakeNode(Concat);
}

long make_node(Nodes *nodes, NodeKind kind, long left, long right) {
    Node node = {
        .kind = kind,
        .left = left,
        .right = right,
    };
    push_node(nodes, node);
    return nodes->len - 1;
}


long make_one_node(Nodes *nodes, NodeKind kind) {
    Node node = {
        .kind = kind,
        .left = 0,
        .right = 0,
    };
    push_node(nodes, node);
    return nodes->len - 1;
}