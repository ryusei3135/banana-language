use std::error::Error;
use std::fmt;

mod parser;
mod regex;
// mod node;

use parser::*;

pub use regex::Regex;
pub use regex::Captures;

// ============================== AST ==============================

#[derive(Debug)]
enum Node {
    Char(char),
    Any,
    Start,
    End,
    Class(Vec<(char, char)>, bool), // (範囲リスト, 否定フラグ)
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat(Box<Node>, usize, Option<usize>), // (中身, min, max)
    Group(Box<Node>, usize),                 // キャプチャ番号 (1始まり)
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

fn match_node(
    node: &Node,
    input: &[char],
    pos: usize,
    caps: &mut Caps,
    k: &mut Cont,
) -> Option<usize> {
    match node {
        Node::Char(c) => {
            if pos < input.len() && input[pos] == *c {
                k(pos + 1, caps)
            } else {
                None
            }
        }

        Node::Any => {
            if pos < input.len() && input[pos] != '\n' {
                k(pos + 1, caps)
            } else {
                None
            }
        }

        Node::Start => {
            if pos == 0 {
                k(pos, caps)
            } else {
                None
            }
        }

        Node::End => {
            if pos == input.len() {
                k(pos, caps)
            } else {
                None
            }
        }

        Node::Class(ranges, negated) => {
            if pos < input.len() {
                let c = input[pos];

                let in_class = ranges
                    .iter()
                    .any(|&(lo, hi)| c >= lo && c <= hi);

                if in_class != *negated {
                    return k(pos + 1, caps);
                }
            }

            None
        }

        Node::Concat(nodes) => {
            match_concat(
                nodes,
                0,
                input,
                pos,
                caps,
                k,
            )
        }

        Node::Alt(branches) => {
            for b in branches {
                let saved = caps.clone();

                if let Some(end) =
                    match_node(b, input, pos, caps, k)
                {
                    return Some(end);
                }

                *caps = saved;
            }

            None
        }

        Node::Group(inner, idx) => {
            let start = pos;

            let mut capture_cont =
                |end: usize, caps: &mut Caps| {
                    caps[*idx] = Some((start, end));
                    k(end, caps)
                };

            match_node(
                inner,
                input,
                pos,
                caps,
                &mut capture_cont,
            )
        }

        Node::Repeat(inner, min, max) => {
            match_repeat(
                inner,
                *min,
                *max,
                input,
                pos,
                caps,
                k,
            )
        }
    }
}

fn match_concat(
    nodes: &[Node],
    i: usize,
    input: &[char],
    pos: usize,
    caps: &mut Caps,
    k: &mut Cont,
) -> Option<usize> {
    if i == nodes.len() {
        return k(pos, caps);
    }

    let mut kk =
        |p: usize, caps: &mut Caps| {
            match_concat(
                nodes,
                i + 1,
                input,
                p,
                caps,
                k,
            )
        };

    match_node(
        &nodes[i],
        input,
        pos,
        caps,
        &mut kk,
    )
}

fn match_repeat(
    inner: &Node,
    min: usize,
    max: Option<usize>,
    input: &[char],
    pos: usize,
    caps: &mut Caps,
    k: &mut Cont,
) -> Option<usize> {
    fn go(
        inner: &Node,
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

            let mut kk =
                |p: usize, caps: &mut Caps| {
                    if p == pos && count >= min {
                        // 空文字マッチで無限ループするのを防ぐ
                        return None;
                    }

                    go(
                        inner,
                        count + 1,
                        min,
                        max,
                        input,
                        p,
                        caps,
                        k,
                    )
                };

            if let Some(end) =
                match_node(
                    inner,
                    input,
                    pos,
                    caps,
                    &mut kk,
                )
            {
                return Some(end);
            }

            *caps = saved;
        }

        if count >= min {
            return k(pos, caps);
        }

        None
    }

    go(
        inner,
        0,
        min,
        max,
        input,
        pos,
        caps,
        k,
    )
}
