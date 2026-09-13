
use regex::{Captures, Regex};
use super::*;
use crate::err;

impl AsmEmitter {
    /// アセンブリ言語のフォーマットをする際に、フォーマット
    /// ファイル側に書いてあるレジスタを埋め込む処理を実行
    pub fn replace_insert_fmt_reg(
        &self, 
        value: &String, 
        size: &types::Size
    ) -> String {
        let insert_reg = Regex::new(r"%\{([^}]+)\}").unwrap();
        let parse_err: Option<err::ErrKind> = None;
        insert_reg
            .replace_all(value, |caps: &Captures| {
                if parse_err.is_some() {
                    // すでにエラーが起きているので、これ以上解析しても意味が無い
                    return String::new();
                }

                let inner = &caps[1].to_string();
                format!(
                    "{}", 
                    self.asm_fmt
                        .get_fmt_reg(
                            &inner.parse::<usize>()
                                .unwrap(), 
                            size
                        )
                )
            })
            .to_owned()
    }
    
    /// レジスタの「番号」だけを一時的に埋め込むための、目印付きの
    /// プレースホルダー文字列を作る。
    ///
    /// 例: `Self::insert_fmt_reg_placeholder(&3)` -> `"{{reg:3}}"`
    ///
    /// このプレースホルダーは、まだ実際のレジスタ名(サイズ込みの
    /// `%eax`のような文字列)へ展開されておらず、後で
    /// [`Self::replace_insert_fmt_reg`]によって展開される。
    pub fn insert_fmt_reg_placeholder(
        reg_num: &usize
    ) -> String {
        format!("%{{{}}}", reg_num)
    }
}
