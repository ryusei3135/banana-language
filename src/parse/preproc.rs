use crate::err::{ErrKind, lex_err::LexErrKind};

use super::*;
use regex::{Captures, Regex};

/// `${...}`を検出する正規表現。
/// `gen_asm_line`は`#asm`ブロックの行ごとに呼ばれるため、毎回
/// `Regex::new`でコンパイルすると無駄なコストがかかる。
/// `OnceLock`で最初の1回だけコンパイルし、以降は使い回す。
// fn inline_var_regex() -> Regex {
//     Regex::new(r"\$\{([^}]+)\}").unwrap()
// }

impl Parser {
    pub(super) fn make_preproc(
        &mut self,
        proc_name: &String,
    ) -> Result<node::Group2Node, err::ErrKind> {
        let result = match proc_name.as_str() {
            /*"define" => {},
            "undef" => {},*/
            "include" => node::Group2Node::Include(self.build_mod_path()?),
            /*"if" => {},
            "ifdef" => {},
            "ifndef" => {},
            "else" => {},
            "elif" => {},
            "endif" => {},
            "error" => {},*/
            "line" => {
                let curr_line = self.build_err_span().line.to_string();
                node::Group2Node::Line(curr_line)
            }
            "preserve" => {
                print::cprint!("%s", proc_name.clone());
                panic!();
            }
            "asm" => self.build_asm_ast()?,
            _ => panic!(),
        };

        Ok(result)
    }

    fn build_mod_path(
        &mut self
    ) -> Result<node::ModPath, err::ErrKind> {
        enum PathTkn {
            Name,
            PathTkn,
        }
        let mut flag: Option<PathTkn> = None;
        let mut mod_path = node::ModPath::new();
        loop {
            match self.next_tkn_ref(vec!["name", "::", ".."])? {
                lex::Tkn::Name(name) => {
                    // 前回のトークンの種類が、無いまたは、"::"の場合だけ実行
                    if flag
                        .as_ref()
                        .is_none_or(|v| matches!(v, PathTkn::PathTkn)) 
                    {
                        mod_path.add_path(&name);
                        flag = Some(PathTkn::Name);
                    } else {
                        // pathの終了
                        break;
                    }
                }
                lex::Tkn::ModPathTkn => {
                    flag = Some(PathTkn::PathTkn);
                }
                _ => {
                    if flag
                        .as_ref()
                        .is_some_and(|v| matches!(v, PathTkn::Name))
                    {
                        break;
                    } else {
                        crate::preproc_err!(self, ExpectedPathSegment);
                    }
                }
            }
            self.next_tkn(vec![]).unwrap();
        }
        Ok(mod_path)
    }

    #[inline(always)]
    fn get_asm_name(
        &mut self
    ) -> Result<String, err::ErrKind> {
        if let lex::Tkn::Name(asm_name) = self.next_tkn(vec!["name"])? 
        {
            Ok(asm_name)
        } else {
            crate::preproc_err!(self, NotFoundAsmName);
        }
    }
    /// ## 戻り値
    /// - Ok inlineアセンブラの名前
    fn build_asm_ast(
        &mut self
    ) -> Result<node::Group2Node, err::ErrKind> {
        
        // #asm(...)なので、(以外が来たらエラー
        if !matches!(
            self.next_tkn(vec!["not `(`"])?, 
            lex::Tkn::LParen
    ) {
            crate::preproc_err!(self, ExpectedLParenAfterAsm);
        }
        let asm_name = self.get_asm_name()?;

        // #asm(...)なので、(以外が来たらエラー
        if !matches!(self.next_tkn(vec![")"])?, lex::Tkn::RParen) {
            crate::preproc_err!(self, ExpectedRParenAfterAsm);
        }
        let nodes = self.gen_asm_preproc()?;
        Ok(node::Group2Node::CompleSyntax((asm_name, nodes)))
    }

    /// アセンブリ言語のプロプロセッサの
    /// 中身(アセンブリ言語本体)を生成する関数
    fn gen_asm_preproc(
        &mut self
    ) -> Result<Vec<node::InlineAsm>, err::ErrKind> {
        let mut nodes = Vec::<node::InlineAsm>::new();

        if matches!(
            self.next_tkn(vec!["{"])?, 
            lex::Tkn::LBrace
        ) {
            let _ = self.next_tkn(vec![])?;
            loop {
                match self
                    .current_tkn()
                    .clone()
                {
                    lex::Tkn::Str(value) => {
                        nodes.push(self.gen_asm_line(&value)?);
                    }
                    lex::Tkn::RBrace => {
                        self.next_tkn(vec![])?;
                        break;
                    }
                    t => {
                        panic!("{:?}", t);
                    }
                }
                self.next_tkn(vec![])?;
            }
            return Ok(nodes);
        }

        Err(err::ErrKind::UnexpectedToken)
    }

    /// inlineアセンブラの1行分の文字列から`node::InlineAsm`を作成する。
    ///
    /// `${...}`は出現順に`{0}`, `{1}`, ...のプレースホルダーへ置き換え、
    /// 中に書かれていた式(変数・構造体のメンバー・ポインタなど、
    /// 普通の式として書けるもの)は`operands`に出現順で積んでいく。
    /// 同じ行に複数の`${...}`があっても、すべて取り込む
    /// (以前の実装は最後の1つしか保持できなかった)。
    fn gen_asm_line(
        &mut self, 
        value: &String
    ) -> Result<node::InlineAsm, err::ErrKind> {
        let inline_var = Regex::new(r"\$\{([^}]+)\}")
            .unwrap(); // 一度だけコンパイルして使い回してOK

        let mut operands = Vec::<node::Expr>::new();
        let mut parse_err: Option<err::ErrKind> = None;

        let asm = inline_var
            .replace_all(
                value, 
                |caps: &Captures|
            {
                if parse_err.is_some() {
                    return String::new();
                }
                println!("{:?}", caps);
                let inner = &caps[1]; // ← 正しく "reg_a" 等が取れる

                // このクロージャの中で別の Regex::new を呼んでも、
                // もう inline_var のノード領域を破壊しない
                match Parser::parse_asm_operand(inner) {
                    Ok(expr) => {
                        let index = operands.len();
                        println!("exprs inline asm {:?}", expr);
                        operands.push(expr);
                        format!("{{{}}}", index)
                    }
                    Err(e) => {
                        parse_err = Some(e);
                        String::new()
                    }
                }
            }
        );
        if let Some(e) = parse_err {
            panic!("KKKKKK {:?}", e);
        }

        Ok(node::InlineAsm { asm, operands })
    }
}

#[cfg(test)]
mod inline_asm_tests {
    use crate::{lex, node, parse};

    /// テスト用に、関数の中に`#asm(...)`ブロックを1つ持つ
    /// プログラムを解析し、`InlineAsm`の一覧を取り出す
    fn gen_inline_asm(asm_body: &str) -> Vec<node::InlineAsm> {
        let src = format!(
            "main(): b1 {{ #asm(gas) {{ {} }} }}", 
            asm_body
        );

        let mut lexer = lex::Lexer::new();
        lexer
            .analy(&src.to_string())
            .unwrap();

        let mut p = parse::Parser::new();
        let nodes = p.parser(lexer.gen_tkns)
            .expect("parse failed");

        let node::Group1Node::FuncDefine(func) = &nodes[0] else {
            panic!("not a func define")
        };

        let node::Group2Node::CompleSyntax(
            (name, lines)
        ) = &func.body[0] else {
            panic!("not an inline asm node: {:?}", func.body[0])
        };
        assert_eq!(name, "gas");
        lines.clone()
    }

    #[test]
    fn plain_var_operand() {
        let lines = gen_inline_asm(r#""mov ${a}, ${b}""#);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].asm, "mov {0}, {1}");
        assert_eq!(
            lines[0].operands,
            vec![
                node::Expr::Var("a".to_string()),
                node::Expr::Var("b".to_string()),
            ]
        );
    }

    #[test]
    fn struct_member_operand() {
        // 構造体のメンバーへのアクセスが、通常の式と同様に使える
        let lines = gen_inline_asm(r#""mov ${p.x}, ${p.y}""#);
        assert_eq!(lines[0].asm, "mov {0}, {1}");
        assert_eq!(
            lines[0].operands,
            vec![
                node::Expr::Member {
                    scope: vec!["p".to_string()],
                    target: Box::new(node::Expr::Var("x".to_string())),
                },
                node::Expr::Member {
                    scope: vec!["p".to_string()],
                    target: Box::new(node::Expr::Var("y".to_string())),
                },
            ]
        );
    }

    #[test]
    fn pointer_deref_and_address_of_operand() {
        // ポインタの参照(`*p`)とアドレス取得(`[p]`)が、通常の式と同様に使える
        let lines = gen_inline_asm(r#""mov ${*p}, ${[p]}""#);
        assert_eq!(lines[0].asm, "mov {0}, {1}");
        assert_eq!(
            lines[0].operands,
            vec![
                node::Expr::ConnectAddr(Box::new(node::Expr::Var("p".to_string()))),
                node::Expr::GetAddress(Box::new(node::Expr::Var("p".to_string()))),
            ]
        );
    }

    #[test]
    fn arithmetic_operand() {
        let lines = gen_inline_asm(r#""mov ${a + 1}, ${p.x * 2}""#);
        assert_eq!(lines[0].asm, "mov {0}, {1}");
        assert_eq!(
            lines[0].operands,
            vec![
                node::Expr::Add((
                    Box::new(node::Expr::Var("a".to_string())),
                    Box::new(node::Expr::Number("1".to_string())),
                )),
                node::Expr::Mul((
                    Box::new(node::Expr::Member {
                        scope: vec!["p".to_string()],
                        target: Box::new(node::Expr::Var("x".to_string())),
                    }),
                    Box::new(node::Expr::Number("2".to_string())),
                )),
            ]
        );
    }

    #[test]
    fn multiple_lines_each_keep_their_own_operands() {
        let lines = gen_inline_asm(r#""mov ${a}, ${b}" "add ${c}, ${d}""#);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].asm, "mov {0}, {1}");
        assert_eq!(lines[1].asm, "add {0}, {1}");
        assert_eq!(
            lines[1].operands,
            vec![
                node::Expr::Var("c".to_string()),
                node::Expr::Var("d".to_string()),
            ]
        );
    }

    #[test]
    fn line_without_any_placeholder_keeps_empty_operands() {
        let lines = gen_inline_asm(r#""nop""#);
        assert_eq!(lines[0].asm, "nop");
        assert!(lines[0].operands.is_empty());
    }
}
