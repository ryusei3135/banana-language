mod assign_var;
mod mem_ir;

use super::*;
use crate::ir;

impl AsmEmitter {
    /// 関数を呼び出す情報がある物を受け取りアセンブリ言語を生成する
    ///
    /// `gen/asm_emitter.rs`の`extract_operand_text`から、関数呼び出しの
    /// 結果を値として使う(戻り値を任意のレジスタへ代入する)際にも
    /// 使われるため`pub(super)`にしている
    pub(super) fn emit_call_func(&mut self, meta_data: &inst::CallFuncMetaData) -> String {
        // 生成するアセンブリコード
        let mut call_func = String::new();

        for (index, param) in meta_data.params.iter().enumerate() {
            // 引数のレジスタを取得
            let param_reg = self.asm_fmt.get_fmt_param::<String>(
                &index, 
                self.curr_inst[*param]
                    .get_param_ty()
                    .unwrap()
            );
            // 引数のレジスタと値のidを挿入
            let src1_idx = if let Some(struct_idx) = self.resolve_struct_idx(param) {
                struct_idx
            } else {
                *param
            };

            // 引数が`Inst::GetAddress`(構造体のポインタを`self`として
            // 渡す場合など、`a.add()`の`self`に相当する`GetAddress(Var(a))`)
            // の場合、渡すべきなのは変数`a`の「値」ではなく「アドレス」
            // である。これを常に`mov`で組み立てると、アドレスを計算せず
            // 値をそのままレジスタへコピーしてしまう
            // (`mov 位置(%rbp), %rdi`のような誤ったコード)。
            // アドレスを求める場合は`mov`ではなく`lea`
            // (テンプレート上のキーは`address`)を使う必要がある。
            let opcode = if self.curr_inst[*param].is_pointer() {
                "address"
            } else {
                "mov"
            };

            let asm = self.asm_fmt
                .get_opcode_tmpl(opcode)
                .replace("{dst}", &param_reg)
                .replace("{src1}", &self.extract_operand_text(&src1_idx, false));
            call_func.push_str(&asm);
        }
        call_func.push_str(&self.asm_fmt.get_call_func_fmt(&meta_data.name));
        call_func
    }

    /// 関数のメタ情報のbodyのデータを元に
    /// アセンブリ言語をフォーマットに沿って生成する関数
    ///
    /// ## 引数
    /// - func_meta_data
    /// - asm_fmt_name
    ///     出力するアセンブリ言語のフォーマットの名前
    pub(super) fn build_func_process(
        &mut self,
        func_meta_data: &mut (String, def_tree::FuncDefInfo),
        asm_fmt_name: &Option<String>,
    ) {
        let this_is_self = func_meta_data.1.first_param_is_self();
        // 新しく関数の作成、
        self.asm_text.push_str(&format!("{}:\n", &func_meta_data.0));
        // 関数ごとにスタックの使用量をリセットする
        // (前の関数の`stk_use_counter`を持ち越すと、この関数の
        //  ローカル変数のオフセットが正しく計算できない)
        self.stk_use_counter = 0;
        if func_meta_data.1.stk_size != 0 {
            // 予約されたサイズ分確保する
            self.asm_text
                .push_str(&self.asm_fmt.gen_stack_frame(func_meta_data.1.stk_size));
        } else {
            if &func_meta_data.0 != "_start" {
                self.asm_text
                    .push_str(self.asm_fmt.func_frame_fmt().as_str());
            }
        }

        self.curr_inst = mem::take(&mut func_meta_data.1.body);

        for node in self.curr_inst.clone().iter() {
            match &node {
                inst::Inst::ExpectJmp(name) => {
                    // 次のフォーマットに使うラベルの名前を予約する
                    if self.reserved_label_name.is_none() {
                        self.reserved_label_name = Some(name.to_string());
                    } else {
                        // 予約するラベルの名前は予約するときにNoneで無ければいけない
                        panic!("system err");
                    }
                }
                inst::Inst::Str { dst, value } => {
                    let label_name = format!("M{}", self.data_idx.to_string());
                    let fmt_data = self.asm_fmt.get_str_fmt(&value, &label_name);
                    self.data_sec_text.push_str(&fmt_data);
                    self.data_map.push((*dst, label_name));
                    self.data_idx += 1;
                }
                inst::Inst::Expr(expr) => {
                    let asm = self.format_expr_inst(&expr);

                    self.asm_text.push_str(&asm);

                    // 式の結果を置いたレジスタを使用中として記録する
                    self.used_reg.mark_used(&self.reg_idx);
                    self.last_inst_idx.push((expr.dst, self.reg_idx));
                    self.reg_idx += 1;
                }
                inst::Inst::Num { .. } => {}
                inst::Inst::InitArr(_) => {}
                inst::Inst::Block(name) => {
                    self.asm_text.push_str(&format!("{}:\n", name));
                }
                inst::Inst::Comple { name, lines } => {
                    if let Some(asm_name) = asm_fmt_name {
                        if name.as_str() == asm_name.as_str() {
                            self.deploy_inline_asm(&name, &lines);
                        } else {
                            panic!("unmatch asm name");
                        }
                    } else {
                        self.deploy_inline_asm(&name, &lines);
                    }
                }
                inst::Inst::Jmp(name) => {
                    self.asm_text.push_str(&format!("jmp {}\n", name));
                }
                inst::Inst::AssignVar { name, dst, value } => {
                    // 代入先(`dst`)が「変数そのもの」ではなく、
                    // 実際に書き込むべきメモリを表すノードを
                    // 参照している場合は、変数への再代入ではなく
                    // そのメモリへ直接値を書き込む必要がある
                    // - `Inst::Pointer`: `[b] = 20`(ポインタの参照先)
                    // - `Inst::InsertArr`: `[arr 0] = 10`(配列の要素)
                    // - `Inst::RefStruct`: `a.[c 0] = 10`(構造体の
                    //   メンバー、あるいはそのメンバーが配列の場合の
                    //   要素へのアクセス)
                    let is_mem_write = matches!(
                        self.curr_inst[*dst],
                        inst::Inst::Pointer(..)
                            | inst::Inst::InsertArr { .. }
                            | inst::Inst::RefStruct { .. }
                    );

                    if is_mem_write {
                        self.write_mem(name, &dst, &value, this_is_self);
                    } else {
                        // 通常の変数への再代入(`b = 10`など)
                        self.update_value_info(&name, &value);

                        let current_reg = self.reg_idx;

                        // 代入先の変数の型(サイズ)を確認し、ポインタ型
                        // であれば、専用のフォーマット(`get_ptr`)で
                        // アドレスのオペランドを組み立てる
                        let text = if self.get_var_ty(&name).is_pointer().is_some() {
                            self.assign_value_ty_is_ptr(current_reg, value, this_is_self)
                        } else {
                            self.assign_value_is_not_ptr(current_reg, value, this_is_self)
                        };

                        if self.expr_vars.iter().find(|v| v == &name).is_some() {
                            self.update_value_reg(&name, &current_reg);
                        }
                        self.asm_text.push_str(&text);
                    }
                }
                inst::Inst::Ret(idx) => {
                    let ret_asm = self.format_line("mov", Some(&0), &idx, None, this_is_self);
                    self.asm_text.push_str(&ret_asm);
                    self.asm_text
                        .push_str(self.asm_fmt.func_frame_end().as_str());
                    self.asm_text.push_str("ret\n");
                }
                inst::Inst::Mov {
                    name,
                    size,
                    dst,
                    src,
                } => {
                    self.mov_value_ir(size, dst, src, &name, this_is_self);
                } // メモリに配置されている値の生成
                inst::Inst::MemoryValue(mem_value) => {
                    // call_func/mem_ir.rs
                    self.mem_value_ir(mem_value, this_is_self);
                }
                inst::Inst::Param(param) => {
                    // 引数の実際の型のサイズを使う
                    // (以前は常にDQ(64bit)決め打ちになっており、例えば
                    //  `int`型の引数でも64bitレジスタとして扱われて
                    //  いたため、後で参照する際のサイズも変数自身の
                    //  型ではなく常に64bitになってしまっていた。
                    //  ポインタ型は`get_fmt_reg`側で自動的に64bit
                    //  レジスタとして扱われるため、ここで特別扱いする
                    //  必要はない)
                    let ty = node.get_param_ty().unwrap();
                    // 引数に使うレジスタを取得する
                    let reg_num = self.asm_fmt.get_fmt_param::<usize>(&param.num, ty.clone());
                    self.insert_var_info(
                        &param.name,
                        asm_emitter::VarIndexInfo::new(&reg_num, &ty, &param.dst),
                    );
                }
                inst::Inst::CallFunc(meta_data) => {
                    // 関数を呼ぶノードが変数に戻り値を代入しないばあいのみ生成
                    if meta_data.parent == ir::IS_NOT_ASSIGN_EXPR {
                        let asm_text = self.emit_call_func(&meta_data);
                        self.asm_text.push_str(asm_text.as_str());
                    }
                }
                /*inst::Inst::Struct { mem, is_self, .. } => {
                    if *is_self {
                        let ini_struct_asm = self.emit_struct_ini_asm(mem.to_vec(), this_is_self);
                        self.asm_text.push_str(&ini_struct_asm);
                    }
                }*/
                _ => {},
            }
        }
        // retがない場合つけたす
        if !self.asm_text.ends_with("ret\n") {
            self.asm_text.push_str("leave\nret\n");
        }
    }
}
