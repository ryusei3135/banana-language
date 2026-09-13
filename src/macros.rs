//use crate::ir::inst;

/// ブロックやラベルなどジャンプ系のノードを生成する
/// ときに使うマクロ
/// ir/builder.rs
#[macro_export]
macro_rules! push_jmp_code {
    ($tree:expr, $variant:ident, $value:expr) => {
        $tree
            .ir_tree
            .push(inst::Inst::$variant(format!("L{}", $value)));
        $tree.id_counter += 1;
    };
}
/// src/lex/pathで使う
#[macro_export]
macro_rules! scope_node {
    ($self:tt, $target:ident, $result:ident, $start:expr) => {
        // "::"をスキップ
        let _ = $self.next_tkn(vec![])?;
        let mut path_node = Vec::<String>::new();
        path_node.push($start.to_string());
        loop {
            // スコープのノードを作成
            let scope_tkn = $self.next_tkn(vec!["name"])?;
            if let lex::Tkn::Name(name) = scope_tkn.clone() {
                path_node.push(name);
            } else {
                return crate::syntax_err!(
                    $self.build_err_span(),
                    crate::err::SyntaxErrKind::ExpectedKind {
                        expected: "name",
                        found: scope_tkn,
                    }
                );
            }

            // スコープやメゾットでなくなったので、ノードを作成
            let after_name = $self.next_tkn_ref(vec![])?;
            if !matches!(after_name, lex::Tkn::$target) {
                let expr = match after_name {
                    // これらのトークンが続く場合のみ、代入/呼び出し/
                    // 初期化の可能性があるので`expr_define_var`に委ねる
                    lex::Tkn::Equal
                    | lex::Tkn::Colon
                    | lex::Tkn::LParen
                    | lex::Tkn::LBrace
                    | lex::Tkn::RBracket => {
                        match
                        $self.expr_define_var(
                            path_node
                            .last()
                            .unwrap()
                            .to_string()
                        )?
                        {
                            node::Expr::Assign { .. } => {
                                node::Expr::Var(path_node.last().unwrap().to_string())
                            }
                            n => {
                                n
                            }
                        }
                    }
                    // それ以外(二項演算子や式の終端記号など)が続く場合、
                    // ここではただ値を読み取るだけなので、`expr_define_var`
                    // を呼んで余分な式を代入として消費してしまわないように
                    // 変数の参照として扱う。ただし、後続の演算子/終端記号
                    // 自身は消費せず、呼び出し元がそれを見て処理を続けられる
                    // ように、名前の次のトークンまでだけ進める
                    _ => {
                        let _ = $self.next_tkn(vec![])?;
                        node::Expr::Var(path_node.last().unwrap().to_string())
                    }
                };
                path_node.pop().unwrap();
                let node = node::Expr::$result {
                    scope: path_node.clone(),
                    target: Box::new(
                        expr
                    )
                };
                return Ok(node);
            }
            let _ = $self.next_tkn(vec![])?;
        }
    };
}

/// asm_settingで使う
/// 構造体のアセンブリ言語を生成する処理を展開する
///
/// ## $name (第三引数)について
/// - `false`: 通常の構造体の初期化。呼び出し元(この関数自身)が
///   新しくスタックフレームを確保するので、各メンバーへは
///   `%rbp`からのオフセット(これまでの`stk_use_counter` + 累積サイズ)
///   で書き込む
/// - `true`: `self`経由の構造体の初期化(例: `Struct::new()`が暗黙的に
///   受け取る自分自身へのポインタに、初期化した値を書き込むケース)。
///   この場合、書き込み先のメモリは呼び出し元が既に確保済みなので、
///   新たにスタックを確保する必要はない。生成されるオペランドの
///   基準レジスタを`%rbp`から第一引数(`self`のポインタ)のレジスタに
///   置き換えるだけで良い(例: `-4(%rbp)` -> `-4(%rdi)`)
#[macro_export]
macro_rules! gen_struct_asm {
    ($self:tt, $struct_node:path, $name:expr, $this_self_ptr:expr) => {
        let mut struct_txt = String::new();
        let mut add_size = 0;
        for member in $struct_node.clone().iter() {
            let inst::MemoryInst::Member {
                value_idx, size, ..
            } = member
            else {
                panic!();
            };

            let value = $self
                .extract_operand_text(&value_idx, $this_self_ptr)
                .to_string();
            // このメンバー分を足した「累積」サイズ
            // (これが、このメンバーの`%rbp`からのオフセットになる)
            add_size += size.to_bytes();

            let offset = if $name {
                // `self`のポインタ先に直接書き込むので、既存の
                // `stk_use_counter`(このスコープでのスタック使用量)は
                // 無関係。累積サイズそのものがオフセットになる
                add_size
            } else {
                // 既に使用していたスタックのサイズ + ここまでの
                // メンバーの累積サイズ = このメンバーの正しいオフセット
                $self.stk_use_counter + add_size
            };

            let fmted = $self.asm_fmt.get_fmt_struct_member(value, &size, &offset);

            if $name {
                // 第一引数(`self`のポインタ)のレジスタを取得し、
                // `%rbp`をそのレジスタに置き換える
                // (ポインタなので64bitのレジスタ(`Size::DQ`)を使う)
                let self_ptr_reg = $self
                    .asm_fmt
                    .get_fmt_reg(&$self.asm_fmt.get_fmt_param::<usize>(&0), &Size::DQ);
                struct_txt.push_str(&fmted.replace("%rbp", &self_ptr_reg));
            } else {
                struct_txt.push_str(&fmted);
            }
        }
        if !$name {
            // 新しくスタックを確保したのは自分自身の場合のみ加算する。
            // (`self`のポインタ先に書き込むだけの場合は、呼び出し元が
            //  既にスタックを確保済みなので、ここで加算してはいけない)
            $self.stk_use_counter += add_size;
        }
        return struct_txt;
    };
}

#[macro_export]
macro_rules! mov_size_fmt {
    ($self:tt, $size:ident) => {
        $self.fmt.fmt.op_size.$size.clone()
    };
}