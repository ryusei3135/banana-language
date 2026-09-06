#include "all.h"


Nodes ini_nodes() {
    static Nodes n = {
        {0},
        0,
        0,
    };
    n.pos = n.nodes;
    return n;
}

void push_node(Nodes *nodes, Node new_node) {
    *nodes->pos = new_node;
    nodes->pos++;
    nodes->len++;
}

long pop_node(Nodes *nodes) {
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

long make_alt_node(Nodes *nodes, long range_idx) {
    Node node = {
        .kind = Alt,
        .left = range_idx,
        .right = 0,
    };
    push_node(nodes, node);
    return nodes->len - 1;
}


long make_concat_node(Nodes *nodes, long range_idx) {
    Node node = {
        .kind = Concat,
        .left = range_idx,
        .right = 0,
    };
    push_node(nodes, node);
    return nodes->len - 1;
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
