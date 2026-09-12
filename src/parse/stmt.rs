//! group1は、関数や構造体など
//! group2は変数の定義や条件分岐など

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum GenFlag {
    Group1,
    Group2,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum StkInfo {
    Generics,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parser {
    gen_nodes: Vec<node::Group1Node>,
    pub(super) tkns: Option<Vec<lex::LocatedTkn>>,
    pub(super) idx: usize,
    pub(super) scope_counter: usize,
    pub(super) struct_self_name: Option<String>,
    pub(super) gen_flag: GenFlag,
    pub(super) other_stk: Vec<(String, StkInfo)>, // 処理中の一時データを保存
}

impl Parser {
    pub fn new() -> Self {
        Self {
            gen_nodes: Vec::new(),
            tkns: None,
            idx: 0,
            scope_counter: 0,
            struct_self_name: None,
            gen_flag: GenFlag::Group1,
            other_stk: Vec::new(),
        }
    }

    // 関数の中身などを作成する
    // P は、この関数が公開されるかどうかのbool
    fn build_func<const P: bool>(&mut self, func_name: &String) -> Result<(), err::ErrKind> {
        // トップレベルの関数定義なので、`Self`が解決される
        // 構造体/列挙型は存在しない
        let node = self.func_node(&func_name, P)?;
        self.gen_nodes.push(node);

        self.gen_flag = GenFlag::Group2;
        self.scope_counter += 1;
        Ok(())
    }

    pub fn parser(
        &mut self,
        tkns: Vec<lex::LocatedTkn>,
    ) -> Result<&Vec<node::Group1Node>, err::ErrKind> {
        self.tkns = Some(tkns);

        loop {
            match &self.gen_flag {
                GenFlag::Group1 => {
                    match self.current_tkn().clone() {
                        lex::Tkn::KeyWordPub => {
                            self.pub_keyword_node()?;
                        }
                        lex::Tkn::Name(name) => {
                            self.build_func::<false>(&name)?;
                        }
                        lex::Tkn::CompleSyn => {
                            let node = self.comple_syntax()?;
                            self.gen_nodes.push(node.change_group1());
                        }
                        lex::Tkn::KeyWordStruct => {
                            let node = self.struct_node()?;
                            self.gen_nodes.push(node);
                        }
                        lex::Tkn::KeyWordEnum => {
                            let node = self.enum_node()?;
                            self.gen_nodes.push(node);
                        }
                        t => panic!("{:?}", t),
                    };
                }
                GenFlag::Group2 => {
                    // 関数の中身が空(`{}`)の場合、`one_line_node`を呼ばずに
                    // ここでスコープを閉じる
                    if self.current_tkn() == &lex::Tkn::RBrace {
                        self.scope_counter -= 1;
                        if self.next_tkn(vec![])
                            .is_err() 
                        {
                            return Ok(&self.gen_nodes);
                        }

                        self.gen_flag = GenFlag::Group1;
                        continue;
                    }

                    let node = self.one_line_node()?;

                    match self.gen_nodes.last_mut().unwrap() {
                        node::Group1Node::FuncDefine(func) => {
                            func.add(node);
                        }
                        t => panic!("KK {:?}", t),
                    }

                    if self.current_tkn() == &lex::Tkn::RBrace {
                        self.scope_counter -= 1;
                        if self.next_tkn(vec![]).is_err() {
                            return Ok(&self.gen_nodes);
                        }

                        self.gen_flag = GenFlag::Group1;
                    }
                    continue;
                }
            }
            if self.next_tkn(vec![]).is_err() && self.scope_counter == 0 {
                return Ok(&self.gen_nodes);
            }
        }
        //Ok(&self.gen_nodes)
    }

    pub(super) fn one_line_node(&mut self) -> Result<node::Group2Node, err::ErrKind> {
        let node = match self.current_tkn().clone() {
            lex::Tkn::CompleSyn => self.comple_syntax()?,
            lex::Tkn::Name(name) => node::Group2Node::Expr(self.build_scope_node(&name)?),
            // ポインタ/配列にアクセスするノードの作成
            lex::Tkn::LBracket => {
                if let lex::Tkn::Name(name) = self.next_tkn(vec!["name"])?.clone() {
                    match self.peek_tkn() {
                        // `name`の次が数字の場合、配列への代入
                        // `[name index] = value`
                        Some(lex::Tkn::Number(_)) => {
                            self.make_array_assign_node(&name)?.wrap_group2()
                        }
                        // それ以外の場合、ポインタへの代入
                        // `[name] = value`
                        _ => {
                            let mut ptr_connect = self.expr_define_var(name.to_string())?;
                            ptr_connect.get_assign_node().name = name.to_string();
                            let dst = ptr_connect.get_assign_node().clone().dst;
                            ptr_connect.get_assign_node().dst =
                                Box::new(node::Expr::ConnectAddr(dst));
                            ptr_connect.wrap_group2()
                        }
                    }
                } else {
                    panic!();
                }
            }
            lex::Tkn::KeyWordRet => node::StmtNode::Return(self.expr_add(true)?).wrap(),
            lex::Tkn::KeyWordLoop => self.make_loop_node()?,
            // 条件分岐
            lex::Tkn::KeyWordCond => {
                let n = self.expr_match()?;
                node::Group2Node::Expr(n)
            }
            lex::Tkn::RBrace => {
                // 空のブロック(`{}`)は呼び出し元(`gen_block_node`や
                // メゾットの本体を解析する処理など)が`one_line_node`を
                // 呼ぶ前に判定するはずなので、ここに来るのは想定外
                panic!("one_line_node: 空のブロックが処理されていません");
            }
            t => {
                panic!("parse stmt {:?}  {:?}", t, self.peek_tkn());
            }
        };
        Ok(node)
    }

    fn pub_keyword_node(
        &mut self
    ) -> Result<(), err::ErrKind> {
        match self.next_tkn(vec!["name", ".."])? {
            lex::Tkn::Name(name) => self.build_func::<true>(&name),
            unexpect_tkn => {
                // 期待したトークンじゃないので、エラー
                crate::syntax_err!(
                    self.build_err_span(),
                    err::SyntaxErrKind::UnexpectTknAfterKeyword {
                        keyword: lex::Tkn::KeyWordPub,
                        expected: vec!["struct", "enum", "name"],
                        found: unexpect_tkn,
                    }
                )
            }
        }
    }

    /// 反復処理のノードを作成する関数
    fn make_loop_node(&mut self) -> Result<node::Group2Node, err::ErrKind> {
        // 反復処理の条件式
        let pattern = match self.next_tkn_ref(vec!["{", ".."])? {
            // "{"の場合は条件無し
            lex::Tkn::LBrace => None,
            // 条件式あり
            _ => {
                // 条件に構造体の初期化を使うことはできない
                Some(Box::new(self.expr_cmp(false)?))
            }
        };
        // "{"をスキップ
        if self.current_tkn() != &lex::Tkn::LBrace {
            panic!();
        }
        self.next_tkn(vec!["{"])?;

        let body = self.gen_block_node()?;
        self.next_tkn(vec!["expr"])?;

        let node = node::Group2Node::Expr(node::Expr::Loop { pattern, body });
        Ok(node)
    }

    /// 同じスコープ内のノードを生成
    pub(super) fn gen_block_node(&mut self) -> Result<Vec<node::Group2Node>, err::ErrKind> {
        let mut block = Vec::<node::Group2Node>::new();

        // ブロックが空(`{}`)の場合、`one_line_node`を呼ばずに
        // そのまま空のブロックを返す
        if self.current_tkn() == &lex::Tkn::RBrace {
            return Ok(block);
        }

        loop {
            let node = self.one_line_node()?;
            block.push(node);

            if self.current_tkn() == &lex::Tkn::RBrace {
                break;
            }
        }
        Ok(block)
    }

    pub(super) fn comple_syntax(&mut self) -> Result<node::Group2Node, err::ErrKind> {
        let lex::Tkn::Name(name) = self.next_tkn(vec![])? else {
            panic!();
        };
        self.make_preproc(&name)
    }

    /// 配列に値を代入するノードを作成する
    ///
    /// ## 呼び出し時の前提
    /// 呼び出し元(`one_line_node`)で`lex::Tkn::LBracket`の次の
    /// `lex::Tkn::Name(name)`まで読み進めた状態で呼び出す。
    /// つまり`current_tkn()`が`name`を指している必要がある。
    ///
    /// ## 文法のルール
    /// `[name index] = value`
    /// - `[arr 0] = 10`
    ///
    /// ## Panics
    /// `name`の次のトークンが数字(`lex::Tkn::Number`)ではない場合panicする
    fn make_array_assign_node(&mut self, name: &String) -> Result<node::Expr, err::ErrKind> {
        // `index`は数字である必要がある。そうでなければpanicする
        let lex::Tkn::Number(index) = self.next_tkn(vec!["number"])? else {
            panic!("配列のインデックスは数字である必要があります");
        };
        self.next_tkn(vec!["]"])?;
        self.next_tkn(vec!["="])?;
        let value = self.expr_branch()?;

        Ok(node::AssignVar::new(
            name,
            node::Expr::RefArray {
                name: name.to_string(),
                dst: Box::new(node::Expr::Var(name.to_string())),
                index: Box::new(node::Expr::Number(index)),
            },
            value,
        ))
    }
}
