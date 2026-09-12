//! inlineアセンブラ(`#asm(...) { "..." }`)の中に出てくる
//! `${...}` の内側を解析するための、専用の式パーサー
//!
//! ## なぜ専用のパーサーが必要か
//! `${x}` や `${x.y}` のような内容は、字句解析すると
//! それだけで完結した(後ろに何も続かない)トークン列になる。
//! これに対し、既存の`one_line_node`/`build_scope_node`などの
//! 文の解析APIは「この式の後には必ずまだトークンが続く」
//! (`;`や`}`など)という前提で先読み(`next_tkn_ref`)している。
//! そのため`${...}`の内容をそれらにそのまま渡すと、
//! 先読みした瞬間にトークンが無く(EOF)、エラーとして
//! 伝播してしまう。

use super::*;

impl Parser {
    /// `${...}` の内側の文字列から式のノードを作成する
    ///
    /// ## 引数
    /// - src `${}`の中に書かれていた文字列(例: `"x.y"`, `"*p"`)
    pub(super) fn parse_asm_operand(
        src: &str
    ) -> Result<node::Expr, err::ErrKind> {
        let mut lexer = lex::Lexer::new();
        lexer.analy(&src.to_string()).unwrap();

        if lexer.gen_tkns.is_empty() {
            return Err(
                crate::preproc_err_at!(
                    err::Span::new(&0, &0), 
                    EmptyAsmOperand
                )
            );
        }

        let mut parser = Parser::new();
        parser.tkns = Some(lexer.gen_tkns);

        let node = parser.asm_operand_expr()?;

        // 式の解析が終わった後もまだトークンが残っている場合は
        // `${...}`の中に、式として解析できない文字列が
        // 混ざっていることになるのでエラーにする
        if parser.peek_tkn().is_some() {
            crate::preproc_err!(parser, UnexpectedTrailingTokenInAsmOperand);
        }

        Ok(node)
    }

    /// 加算・減算を含む式のエントリーポイント
    pub(super) fn asm_operand_expr(
        &mut self
    ) -> Result<node::Expr, err::ErrKind> {
        self.asm_operand_add()
    }

    fn asm_operand_add(&mut self) -> Result<node::Expr, err::ErrKind> {
        let mut left = self.asm_operand_mul()?;

        loop {
            left = match self.current_tkn() {
                lex::Tkn::Add => {
                    self.advance_tkn();
                    node::Expr::Add(node::Expr::wrap(left, self.asm_operand_mul()?))
                }
                lex::Tkn::Sub => {
                    self.advance_tkn();
                    node::Expr::Sub(node::Expr::wrap(left, self.asm_operand_mul()?))
                }
                _ => break,
            };
        }

        Ok(left)
    }

    fn asm_operand_mul(
        &mut self
    ) -> Result<node::Expr, err::ErrKind> {
        let mut left = self.asm_operand_unary()?;

        loop {
            left = match self.current_tkn() {
                lex::Tkn::Mul => {
                    self.advance_tkn();
                        node::Expr::Mul(
                            node::Expr::wrap(
                            left, 
                            self.asm_operand_unary()?
                        )
                    )
                }
                lex::Tkn::Div => {
                    self.advance_tkn();
                    node::Expr::Div(node::Expr::wrap(left, self.asm_operand_unary()?))
                }
                _ => break,
            };
        }

        Ok(left)
    }

    /// ポインタの参照(`*p`)とアドレス取得(`[p]`)を含む単項式
    fn asm_operand_unary(
        &mut self
    ) -> Result<node::Expr, err::ErrKind> {
        match self.current_tkn().clone() {
            // `*p` ポインタ`p`が指す値を読み取る
            lex::Tkn::Mul => {
                self.advance_tkn();
                Ok(node::Expr::ConnectAddr(Box::new(self.asm_operand_unary()?)))
            }
            // `[p]` 変数`p`のアドレスを取得する
            lex::Tkn::LBracket => {
                self.advance_tkn();
                let inner = self.asm_operand_add()?;

                if !matches!(self.current_tkn(), lex::Tkn::RBracket) {
                    crate::preproc_err!(self, ExpectedRBracketInAsmOperand);
                }
                self.advance_tkn();

                Ok(node::Expr::GetAddress(Box::new(inner)))
            }
            _ => self.asm_operand_primary(),
        }
    }

    fn asm_operand_primary(
        &mut self
    ) -> Result<node::Expr, err::ErrKind> {
        match self.current_tkn().clone() {
            lex::Tkn::Number(value) => {
                self.advance_tkn();
                Ok(node::Expr::Number(value))
            }
            lex::Tkn::Str(value) => {
                self.advance_tkn();
                Ok(node::Expr::Str(value))
            }
            lex::Tkn::Name(name) => {
                self.advance_tkn();
                self.asm_operand_name_tail(name)
            }
            lex::Tkn::LParen => {
                self.advance_tkn();
                let inner = self.asm_operand_add()?;

                if !matches!(self.current_tkn(), lex::Tkn::RParen) {
                    crate::preproc_err!(self, ExpectedRParenInAsmOperand);
                }
                self.advance_tkn();

                Ok(inner)
            }
            _ => {
                crate::preproc_err!(self, UnexpectedTokenInAsmOperand);
            }
        }
    }

    /// 変数名の後に続く、構造体のメンバーアクセス(`.field`)を解析する
    /// - `x`   -> 通常の変数の参照
    /// - `x.y` -> 構造体`x`のメンバー`y`への参照
    fn asm_operand_name_tail(
        &mut self, 
        name: String
    ) -> Result<node::Expr, err::ErrKind> {
        if matches!(self.current_tkn(), lex::Tkn::Dot) {
            self.advance_tkn();

            let lex::Tkn::Name(field) = self
                .current_tkn()
                .clone() else 
            {
                crate::preproc_err!(
                    self, 
                    ExpectedMemberNameInAsmOperand
                );
            };
            self.advance_tkn();

            Ok(node::Expr::Member {
                scope: vec![name],
                target: Box::new(node::Expr::Var(field)),
            })
        } else {
            Ok(node::Expr::Var(name))
        }
    }
}
