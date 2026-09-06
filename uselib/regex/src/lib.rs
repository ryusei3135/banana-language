use std::error::Error;
use std::fmt;

mod regex;

pub use regex::Captures;
pub use regex::Regex;

// ============================== AST ==============================

#[repr(C)]
pub enum NodeKind {
    Char,
    Any,
    Start,
    End,
    Class,  // left = レンジ一覧(Rangeノード)のindex, right = 否定フラグ
    Concat, // left = 子ノード列(Rangeノード)のindex
    Alt,    // left = 子ノード列(Rangeノード)のindex
    Repeat, // left = 中身のindex, right = (min,max)を持つRangeノードのindex
    Group,  // left = 中身のindex, right = キャプチャ番号 (1始まり)

    Range, // l = start, r = end
    Nodes, // is ptr
    Flag,  // r = bool
}

#[repr(C)]
pub struct Node {
    pub kind: NodeKind,
    pub left: i64,
    pub right: i64,
}

#[repr(C)]
pub struct Nodes {
    pub nodes: [Node; 2048],
    pub pos: *mut Node,
    pub len: i64,
    pub max_len: i64,
}

// ============================== エラー ==============================

#[derive(Debug)]
pub struct RegexError(String);

impl fmt::Display for RegexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "regex parse error: {}", self.0)
    }
}

impl Error for RegexError {}

#[link(name = "regex", kind = "static")]
unsafe extern "C" {
    pub fn is_byte_digit(chr: u8) -> u8;
}

// ============================== マッチング (継続渡しバックトラック) ==============================

type Caps = Vec<Option<(usize, usize)>>;
type Cont<'a> = dyn FnMut(usize, &mut Caps) -> Option<usize> + 'a;

/// `Range` ノード (left=start, right=end) が指す、アリーナ上で連続する
/// 子ノードの index 列 [start, end) を取り出す小さなヘルパー。
fn range_bounds(nodes: &Nodes, range_idx: i64) -> (i64, i64) {
    let r = &nodes.nodes[range_idx as usize];
    (r.left, r.right)
}

fn match_node(
    nodes: &Nodes,
    idx: i64,
    input: &[char],
    pos: usize,
    caps: &mut Caps,
    k: &mut Cont,
) -> Option<usize> {
    let node = &nodes.nodes[idx as usize];

    match node.kind {
        NodeKind::Char => {
            let c = (node.left as u8) as char;
            if pos < input.len() && input[pos] == c {
                k(pos + 1, caps)
            } else {
                None
            }
        }

        NodeKind::Any => {
            if pos < input.len() && input[pos] != '\n' {
                k(pos + 1, caps)
            } else {
                None
            }
        }

        NodeKind::Start => {
            if pos == 0 {
                k(pos, caps)
            } else {
                None
            }
        }

        NodeKind::End => {
            if pos == input.len() {
                k(pos, caps)
            } else {
                None
            }
        }

        NodeKind::Class => {
            if pos < input.len() {
                let c = input[pos];
                let negated = node.right != 0;
                let (start, end) = range_bounds(nodes, node.left);

                let in_class = (start..end).any(|i| {
                    let pair = &nodes.nodes[i as usize];
                    let lo = (pair.left as u8) as char;
                    let hi = (pair.right as u8) as char;
                    c >= lo && c <= hi
                });

                if in_class != negated {
                    return k(pos + 1, caps);
                }
            }

            None
        }

        NodeKind::Concat => {
            let (start, end) = range_bounds(nodes, node.left);
            match_concat(nodes, start, end, input, pos, caps, k)
        }

        NodeKind::Alt => {
            let (start, end) = range_bounds(nodes, node.left);

            for i in start..end {
                let saved = caps.clone();

                if let Some(end) = match_node(nodes, i, input, pos, caps, k) {
                    return Some(end);
                }

                *caps = saved;
            }

            None
        }

        NodeKind::Group => {
            let inner = node.left;
            let group_idx = node.right as usize;
            let start = pos;

            let mut capture_cont = |end: usize, caps: &mut Caps| {
                caps[group_idx] = Some((start, end));
                k(end, caps)
            };

            match_node(nodes, inner, input, pos, caps, &mut capture_cont)
        }

        NodeKind::Repeat => {
            let inner = node.left;
            let (min_raw, max_raw) = range_bounds(nodes, node.right);
            let min = min_raw as usize;
            let max = if max_raw < 0 { None } else { Some(max_raw as usize) };

            match_repeat(nodes, inner, min, max, input, pos, caps, k)
        }

        // Range / Nodes / Flag は他ノードの補助データであり、
        // それ単体がマッチング対象になることはない。
        NodeKind::Range | NodeKind::Nodes | NodeKind::Flag => None,
    }
}

fn match_concat(
    nodes: &Nodes,
    i: i64,
    end: i64,
    input: &[char],
    pos: usize,
    caps: &mut Caps,
    k: &mut Cont,
) -> Option<usize> {
    if i == end {
        return k(pos, caps);
    }

    let mut kk = |p: usize, caps: &mut Caps| match_concat(nodes, i + 1, end, input, p, caps, k);

    match_node(nodes, i, input, pos, caps, &mut kk)
}

fn match_repeat(
    nodes: &Nodes,
    inner: i64,
    min: usize,
    max: Option<usize>,
    input: &[char],
    pos: usize,
    caps: &mut Caps,
    k: &mut Cont,
) -> Option<usize> {
    fn go(
        nodes: &Nodes,
        inner: i64,
        count: usize,
        min: usize,
        max: Option<usize>,
        input: &[char],
        pos: usize,
        caps: &mut Caps,
        k: &mut Cont,
    ) -> Option<usize> {
        // 貪欲マッチ:
        // まず「もう1回繰り返す」方を先に試す
        if max.map_or(true, |m| count < m) {
            let saved = caps.clone();

            let mut kk = |p: usize, caps: &mut Caps| {
                if p == pos && count >= min {
                    // 空文字マッチで無限ループするのを防ぐ
                    return None;
                }

                go(nodes, inner, count + 1, min, max, input, p, caps, k)
            };

            if let Some(end) = match_node(nodes, inner, input, pos, caps, &mut kk) {
                return Some(end);
            }

            *caps = saved;
        }

        if count >= min {
            return k(pos, caps);
        }

        None
    }

    go(nodes, inner, 0, min, max, input, pos, caps, k)
}