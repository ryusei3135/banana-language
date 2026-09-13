use super::*;

impl Parser {
    /// モジュールのノードを作成
    /// name::mod
    pub(super) fn build_scope_node(
        &mut self, 
        name: &String
    ) -> Result<node::Expr, err::ErrKind> {
        if matches!(self.next_tkn_ref(vec!["{"])?, lex::Tkn::LBrace) {
            self.next_tkn(vec![])?;
            let node = self.struct_init_node::<false>(name);
            self.next_tkn(vec![])?;
            return node;
        }
        if matches!(self.next_tkn_ref(vec!["."])?, lex::Tkn::Dot) {
            return self.build_member_node(&name);
        }
        // "::"がないので、何も返さない
        if !matches!(self.next_tkn_ref(vec!["not `::`"])?, lex::Tkn::ModPathTkn) {
            return Ok(self.expr_define_var(name.to_string())?);
        }

        crate::scope_node!(self, ModPathTkn, Scope, &name);
    }

    /// メゾットなどのノードを作成
    /// name.method
    #[inline(always)]
    pub(super) fn build_member_node(
        &mut self, 
        name: &String
    ) -> Result<node::Expr, err::ErrKind> {
        // "."がないので、何も返さない
        if !matches!(self.next_tkn_ref(vec!["not `.`"])?, lex::Tkn::Dot) {
            return Ok(self.expr_define_var(name.to_string())?);
        }

        // "."の次のトークンを確認するため一旦"."まで進める
        self.next_tkn(vec!["."])?;
        let after_dot_is_bracket =
            matches!(self.next_tkn_ref(vec!["name", "["])?, lex::Tkn::LBracket);
        // まだ"."を消費していない状態(呼び出し時点の位置)に戻す
        self.back_tkn();

        // "."の次が"["の場合、構造体の配列メンバーの要素に
        // アクセス(または代入)するノードを作成する: `name.[member index]`
        if after_dot_is_bracket {
            // "."をスキップ
            self.next_tkn(vec!["."])?;
            // "["をスキップ
            self.next_tkn(vec!["["])?;
            return self.build_member_array_node(name);
        }

        crate::scope_node!(self, Dot, Member, &name);
    }

    /// 構造体の配列メンバーの要素にアクセス、または代入するノードを作成する
    /// - 読み取り: `name.[member index]`
    /// - 代入:     `name.[member index] = value`
    ///
    /// ## 呼び出し時の前提
    /// 呼び出し元(`build_member_node`)で、`.`と`[`を読み飛ばした
    /// 状態で呼び出す。つまり`current_tkn()`が`[`を指している必要がある。
    ///
    /// ## Errors
    /// `member`の次のトークンが名前(`lex::Tkn::Name`)、または
    /// その次が数字(`lex::Tkn::Number`)ではない場合エラー
    fn build_member_array_node(
        &mut self, 
        name: &String
    ) -> Result<node::Expr, err::ErrKind> {
        let member_tkn = self.next_tkn(vec!["name"])?;
        let lex::Tkn::Name(member) = member_tkn.clone() else {
            return crate::syntax_err!(
                self.build_err_span(),
                err::SyntaxErrKind::ExpectedKind {
                    expected: "name",
                    found: member_tkn,
                }
            );
        };
        let index_tkn = self.next_tkn(vec!["number"])?;
        let lex::Tkn::Number(index) = index_tkn.clone() else {
            return crate::syntax_err!(
                self.build_err_span(),
                err::SyntaxErrKind::ExpectedKind {
                    expected: "number",
                    found: index_tkn,
                }
            );
        };
        // "]"まで進める(current_tkn()は"]"を指す)
        self.next_tkn(vec!["]"])?;

        let member_node = node::Expr::Member {
            scope: vec![name.to_string()],
            target: Box::new(node::Expr::RefArray {
                name: member.clone(),
                dst: Box::new(node::Expr::Var(member)),
                index: Box::new(node::Expr::Number(index)),
            }),
        };

        // `a.[c 0] = 10`のように代入の場合、"="の後に続く値を
        // 読み取り、代入のノードとして返す
        if matches!(self.next_tkn_ref(vec!["="])?, lex::Tkn::Equal) {
            self.next_tkn(vec!["="])?;
            let value = self.expr_branch()?;
            return Ok(node::AssignVar::new(name, member_node, value));
        }

        // 代入ではなく値の参照なので、"]"を消費せずに返す
        // (呼び出し元の`expr_value`が続けて読み進める)
        Ok(member_node)
    }
}