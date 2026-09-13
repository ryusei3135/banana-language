use super::{Parser, *};

/// 関数を定義するノードの生成
impl Parser {
    /// この関数を呼び出すときは、次のトークンが`(`で無ければ
    /// いけない
    ///
    /// ## self_name
    /// 構造体/列挙型に定義されたメゾットを解析している場合、その
    /// 構造体/列挙型自身の名前を渡す。引数や戻り値の型に予約語
    /// `Self`が使われたとき、この名前へ解決するために使われる。
    /// トップレベルの関数を解析している場合は`None`を渡す。
    pub(super) fn func_node(
        &mut self,
        func_name: &String,
        is_public: bool,
    ) -> Result<node::Group1Node, err::ErrKind> {
        let arg = match self.next_tkn(vec!["(", "<"])? {
            lex::Tkn::LParen => self.define_arg_node()?,
            lex::Tkn::LAngleBracket => {
                // ジェネリクス(`<..>`)はまだパーサーが対応していない
                return crate::syntax_err!(
                    self.build_err_span(),
                    err::SyntaxErrKind::NotImplemented {
                        feature: "ジェネリクス",
                    }
                );
            }
            t => {
                return crate::syntax_err!(
                    self.build_err_span(),
                    err::SyntaxErrKind::ExpectedKind {
                        expected: "( or <",
                        found: t,
                    }
                );
            }
        };

        match self.current_tkn() {
            lex::Tkn::Colon => {
                let ret_ty = self.define_ty_node()?;

                if self.current_tkn() == &lex::Tkn::LBrace {
                    Ok(node::FuncDefine::new(
                        func_name.clone(),
                        arg,
                        ret_ty,
                        is_public,
                    ))
                } else {
                    Err(err::ErrKind::NotFoundTkn(lex::Tkn::LBrace))
                }
            }
            // 戻り値の型が指定されていない場合、組み込みの`int`型を
            // デフォルトの戻り値の型として扱う
            lex::Tkn::LBrace => Ok(node::FuncDefine::new(
                func_name.clone(),
                arg,
                node::TyNode::Ty("int".to_string()),
                is_public,
            )),
            t => crate::syntax_err!(
                self.build_err_span(),
                err::SyntaxErrKind::ExpectedKind {
                    expected: ": or `{`",
                    found: t.clone(),
                }
            ),
        }
    }

    /// 引数の定義をするノードを作成する
    /// この関数では、型のあとまでトークンを進めているので、呼び出し元では
    /// current_tknでトークンを判定する
    ///
    /// ## Errors
    /// - `,`を挟まずに次の引数の定義が来た場合(名前や`)`以外のトークン)
    /// - `,`の直前に引数が無い場合(先頭が`,`、または`,,`)
    /// - 引数の後に`,`でも`)`でもないトークンが来た場合
    ///
    /// ## self_name
    /// メゾットの引数を解析している場合、そのメゾットが定義されている
    /// 構造体/列挙型自身の名前を渡す。引数の型に予約語`Self`が使われた
    /// 場合、`node::TyNode::SelfTy(self_name)`へ解決するために使われる
    /// (実際に`Self`が第一引数以外に使われていないかのチェックは、
    /// IRへの変換時に行う)
    fn define_arg_node(
        &mut self
    ) -> Result<Vec<node::ArgsNode>, err::ErrKind> {
        if self.current_tkn() != &lex::Tkn::LParen {
            return Err(err::ErrKind::NotFoundTkn(lex::Tkn::LParen));
        }

        let mut args_params = Vec::<node::ArgsNode>::new();
        // 引数の定義を作成可能か
        let mut can_create_param = true;

        loop {
            // ループの先頭に来る時点では、直前の反復で`,`を消費して
            // `can_create_param`が`true`に戻されているはず(そうでなければ
            // 下の`,`の分岐、またはループを抜ける`)`の分岐のいずれかを
            // 通っているはず)なので、ここが`false`になるのはパーサー
            // 自体のバグ
            if !can_create_param {
                unreachable!("define_arg_node: 引数リストの内部状態が不正です");
            }
            match self.next_tkn(vec!["name", ")"])? {
                lex::Tkn::Name(name) => {
                    // 直前の`if !can_create_param`のチェックを通過して
                    // いるため、ここでは常に`true`(同上、パーサー自体の
                    // バグでない限り到達しない)
                    if !can_create_param {
                        unreachable!("define_arg_node: 引数リストの内部状態が不正です");
                    }
                    self.next_tkn(vec![])?;
                    let ty = self.define_ty_node()?;
                    args_params.push(node::ArgsNode {
                        name: name.clone(),
                        ty,
                    });
                    can_create_param = false;
                }
                // これが来た場合引数の定義が終了
                lex::Tkn::RParen => {
                    self.next_tkn(vec![])?;
                    break;
                }
                t => {
                    return crate::syntax_err!(
                        self.build_err_span(),
                        err::SyntaxErrKind::ExpectedKind {
                            expected: "name or `)`",
                            found: t,
                        }
                    );
                }
            }

            match self.current_tkn() {
                lex::Tkn::RParen => {
                    self.next_tkn(vec![])?;
                    break;
                }
                lex::Tkn::Comma => {
                    if !can_create_param {
                        can_create_param = true;
                    } else {
                        // `,`の前に引数が無い(先頭が`,`、または`,,`)
                        return crate::syntax_err!(
                            self.build_err_span(),
                            err::SyntaxErrKind::ExpectedKind {
                                expected: "name",
                                found: lex::Tkn::Comma,
                            }
                        );
                    }
                }
                t => {
                    return crate::syntax_err!(
                        self.build_err_span(),
                        err::SyntaxErrKind::ExpectedKind {
                            expected: ", or `)`",
                            found: t.clone(),
                        }
                    );
                }
            }
        }

        Ok(args_params)
    }
}