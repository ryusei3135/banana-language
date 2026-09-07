use super::*;
use std::ffi::{CStr, CString};



#[repr(C)]
pub struct Parser {
    pub chars: *mut u8,
    pub chars_len: usize,
    pub pos: usize,
    pub group_count: usize,
}

#[repr(C)]
pub enum OpKind {
    NONE,
    SOME,
}

#[repr(C)]
pub enum ResultKind {
    Ok,
    Err,
}

#[repr(C)]
pub union NodeVal {
    pub ok: i64,
    pub err: *mut u8,
}

#[repr(C)]
pub struct NodeResult {
    pub v: NodeVal,
    pub kind: ResultKind,
}


unsafe extern "C" {
    pub fn ini_nodes() -> &'static mut Nodes;

    pub fn parse_new(pattern: *const u8, len: i64) -> Parser;
    pub fn parse_alt(p: *mut Parser, n: *mut Nodes) -> NodeResult;
    pub fn parser_drop(p: *mut Parser);
}


pub struct Regex {
    nodes: &'static Nodes,
    root: i64,
    group_count: usize,
}

impl Regex {
    /// 正規表現パターンをコンパイルする。
    pub fn new(pattern: &str) -> Result<Regex, RegexError> {
        unsafe {
            let c_pattern = CString::new(pattern).map_err(|e| {
                RegexError(format!("パターンに NUL 文字が含まれています: {}", e))
            })?;

            let mut parser: Parser = parse_new(
                c_pattern.as_ptr() as *const u8, 
                pattern.bytes().len() as i64
            );
            let nodes: &'static mut Nodes = ini_nodes();
            let result = parse_alt(&mut parser, nodes);
            parser_drop(&mut parser);

            let root = match result.kind {
                ResultKind::Ok => result.v.ok,
                ResultKind::Err => return Err(RegexError(cstr_to_string(result.v.err))),
            };

            if parser.pos < parser.chars_len {
                return Err(RegexError(format!(
                    "予期しない文字が {} 文字目にあります",
                    parser.pos
                )));
            }

            Ok(Regex {
                nodes,
                root,
                group_count: parser.group_count,
            })
        }
    }

    /// `chars` の `start` 文字目以降で最初にマッチする位置を探す。
    /// 見つかれば各キャプチャグループの (開始, 終了) 文字インデックスを返す。
    /// 添字 0 が全体マッチに対応する。
    fn find_at(&self, chars: &[char], start: usize) -> Option<Caps> {
        for pos in start..=chars.len() {
            let mut caps: Caps = vec![None; self.group_count + 1];
            let mut k: Box<Cont> = Box::new(|end, _caps: &mut Caps| Some(end));
            if let Some(end) = match_node(self.nodes, self.root, chars, pos, &mut caps, &mut *k) {
                caps[0] = Some((pos, end));
                return Some(caps);
            }
        }
        None
    }

    /// マッチした箇所をすべて `replacer` に置き換えた新しい文字列を返す。
    /// `replacer` には次の2種類を渡せる:
    ///   - `&str` : `$1`, `$2`, ... / `$$` を使ったテンプレート (`Captures::replace_all` に委譲)
    ///   - `FnMut(&Captures) -> String` のクロージャ : マッチ毎に呼ばれ、戻り値で置き換える
    pub fn replace_all<R: Replacer>(&self, text: &str, mut replacer: R) -> String {
        let chars: Vec<char> = text.chars().collect();
        let char_to_byte = build_char_to_byte(text);

        let mut result = String::new();
        let mut last_end = 0usize; // 文字インデックス
        let mut search_pos = 0usize;

        while search_pos <= chars.len() {
            let spans = match self.find_at(&chars, search_pos) {
                Some(s) => s,
                None => break,
            };
            let (m_start, m_end) = spans[0].unwrap();

            // マッチの直前までの部分をそのままコピー
            result.push_str(&text[char_to_byte[last_end]..char_to_byte[m_start]]);

            // このマッチのキャプチャから置換文字列を組み立てる
            let captures = Captures::new(text, spans);
            result.push_str(&replacer.replace(&captures));

            last_end = m_end;
            search_pos = if m_end > m_start { m_end } else { m_end + 1 };
        }

        result.push_str(&text[char_to_byte[last_end]..]);
        result
    }
}

/// C 側から渡ってきたエラーメッセージ (`*const u8`, NUL終端) を String に変換する。
/// `parse_class_char` / `parse_bound` などの Err メッセージは全て C 文字列リテラルなので、
/// ここで一箇所にまとめて安全に扱う。
unsafe fn cstr_to_string(ptr: *const u8) -> String {
    if ptr.is_null() {
        return String::from("不明なエラーです");
    }
    CStr::from_ptr(ptr as *const i8)
        .to_string_lossy()
        .into_owned()
}

/// `Regex::replace_all` に渡せる「置換の仕方」を表すトレイト。
/// 次の2つに対して実装されている:
///   - `&str`                      : `Captures::replace_all` によるテンプレート展開
///   - `FnMut(&Captures) -> String` : マッチ毎に呼ばれるクロージャ
pub trait Replacer {
    fn replace(&mut self, caps: &Captures) -> String;
}

impl<'r> Replacer for &'r str {
    fn replace(&mut self, caps: &Captures) -> String {
        caps.replace_all(self)
    }
}

impl<F> Replacer for F
where
    F: FnMut(&Captures) -> String,
{
    fn replace(&mut self, caps: &Captures) -> String {
        self(caps)
    }
}

/// 文字インデックス -> バイトオフセットの対応表を作る (&str のスライスに必要)。
fn build_char_to_byte(text: &str) -> Vec<usize> {
    let mut table = Vec::with_capacity(text.len() + 1);
    let mut byte_pos = 0usize;
    for c in text.chars() {
        table.push(byte_pos);
        byte_pos += c.len_utf8();
    }
    table.push(byte_pos);
    table
}

/// 1回のマッチのキャプチャ結果。`new` と `replace_all` のみ実装。
///
/// 添字 0 は全体マッチ、1以降は `(...)` の出現順に対応する。
/// マッチしなかった任意グループの位置は `None` になる。
pub struct Captures<'t> {
    text: &'t str,
    // 各キャプチャグループの (開始, 終了) 文字インデックス。0番目が全体マッチ。
    spans: Vec<Option<(usize, usize)>>,
    char_to_byte: Vec<usize>,
}

impl<'t> Captures<'t> {
    /// マッチ対象のテキスト全体と、各グループの (開始, 終了) 文字インデックスから
    /// `Captures` を組み立てる。`spans[0]` は全体マッチの範囲。
    pub fn new(text: &'t str, spans: Vec<Option<(usize, usize)>>) -> Captures<'t> {
        Captures { char_to_byte: build_char_to_byte(text), text, spans }
    }

    /// 添字 `i` のキャプチャの実際の文字列を返す。
    /// マッチしなかったグループ、または存在しない添字は `None`。
    fn get(&self, i: usize) -> Option<&'t str> {
        let (s, e) = (*self.spans.get(i)?)?;
        Some(&self.text[self.char_to_byte[s]..self.char_to_byte[e]])
    }

    /// `template` 中の `$1`, `$2`, ... を対応するキャプチャの文字列に、
    /// `$$` をリテラルの `$` に置き換えた文字列を返す。
    /// マッチしなかったグループを参照した場合は空文字列に置き換わる。
    pub fn replace_all(&self, template: &str) -> String {
        let chars: Vec<char> = template.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '$' && i + 1 < chars.len() {
                if chars[i + 1] == '$' {
                    out.push('$');
                    i += 2;
                    continue;
                }
                if unsafe { is_byte_digit(chars[i + 1] as u8) == 1 } {
                    let mut j = i + 1;
                    let mut num = String::new();
                    while j < chars.len() && unsafe { is_byte_digit(chars[j] as u8) == 1 } {
                        num.push(chars[j]);
                        j += 1;
                    }
                    let idx: usize = num.parse().unwrap();
                    if let Some(s) = self.get(idx) {
                        out.push_str(s);
                    }
                    i = j;
                    continue;
                }
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    }
}

/// `caps[1]` のように添字アクセスできるようにする。
/// マッチしなかったグループや範囲外の添字を指定するとパニックする
/// (`regex` クレートの `Captures` と同じ挙動)。
impl<'t> std::ops::Index<usize> for Captures<'t> {
    type Output = str;
    fn index(&self, i: usize) -> &str {
        self.get(i)
            .unwrap_or_else(|| panic!("キャプチャグループ {} はマッチしていません", i))
    }
}