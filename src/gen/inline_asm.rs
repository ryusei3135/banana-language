use super::*;

impl AsmEmitter {
    /// 渡されたテンプレート文字列(オペランドの`{0}`などを置換する前の、
    /// 生のインラインアセンブラの1行)の中に、直接書かれているレジスタ名
    /// (`%rax`など)を検出し、対応する内部のレジスタ番号を返す。
    fn detect_literal_regs(&self, template: &str) -> Vec<usize> {
        let mut found = Vec::new();
        for (reg_idx, reg_name) in self.asm_fmt.all_reg_names() {
            if template.contains(reg_name.as_str()) && !found.contains(&reg_idx) {
                found.push(reg_idx);
            }
        }
        found
    }

    /// 指定したレジスタ番号を、現在使用中の値として保持している変数を探す。
    /// (`(変数名, その変数の型のサイズ)`を返す。見つからなければ`None`)
    fn var_using_reg(&self, reg: &usize) -> Option<(String, Size)> {
        self.var_hash_map
            .iter()
            .find(|(_, info)| !info.is_stack && &info.reg == reg)
            .map(|(name, info)| (name.clone(), info.size.clone()))
    }

    /// `exclude`(インラインアセンブラ自身が使うレジスタ)を除いた上で、
    /// 現在どの変数にも使われていない、空いているレジスタ番号を1つ探す。
    fn find_free_reg(&self, exclude: &[usize]) -> Option<usize> {
        let used = self.used_reg.used_regs();
        (0..self.asm_fmt.reg_count()).find(|reg| !used.contains(reg) && !exclude.contains(reg))
    }

    /// 渡されたノードが変数(`AssignVar`/`Param`)をレジスタとして
    /// 参照するオペランドである場合、その変数自身の型のサイズを返す。
    /// (インラインアセンブラのdstオペランドのサイズを求めるために使う。
    ///  `Pointer`/`GetAddress`でラップされている場合は、その内側を
    ///  辿って判定する)
    fn resolve_operand_var_size(&self, node_idx: &usize) -> Option<Size> {
        match self.curr_inst[*node_idx].clone() {
            inst::Inst::AssignVar { name, .. } => {
                self.var_hash_map.get(&name).map(|v| v.size.clone())
            }
            inst::Inst::Param(param) => {
                self.var_hash_map.get(&param.name).map(|v| v.size.clone())
            }
            inst::Inst::Pointer(inner) | inst::Inst::GetAddress(inner) => {
                self.resolve_operand_var_size(&inner)
            }
            _ => None,
        }
    }

    /// `extract_operand_text`と同様にオペランドの文字列を組み立てるが、
    /// レジスタとして参照される変数(`AssignVar`/`Param`)の場合、
    /// その変数自身のサイズではなく`forced_size`を優先して使う。
    ///
    /// インラインアセンブラでは、`mov {0}, {1}`のように異なる変数を
    /// 組み合わせて書くことがあるが、AT&T記法では`mov`などの
    /// オペランドは同じ幅のレジスタでなければ正しいアセンブリに
    /// ならない。そのため、srcのレジスタサイズは常にdstのレジスタ
    /// サイズに合わせて上書きする必要がある
    /// (ポインタ型の変数は、常にアドレス=64bitとして扱われるため
    ///  この上書きの対象外)
    fn extract_operand_text_sized(
        &mut self,
        node_idx: &usize,
        in_self_ptr: bool,
        forced_size: &Size,
    ) -> String {
        match self.curr_inst[*node_idx].clone() {
            inst::Inst::AssignVar { name, .. } => {
                let var_info = self.var_hash_map.get(&name).unwrap();
                let size = if var_info.size.is_pointer().is_some() {
                    Size::DQ
                } else {
                    forced_size.clone()
                };
                self.asm_fmt.get_fmt_reg(&var_info.reg, &size)
            }
            inst::Inst::Param(param) => {
                let var_info = self.var_hash_map.get(&param.name).unwrap();
                if let Some(ty) = var_info.size.is_pointer() {
                    let reg = self.asm_fmt.get_fmt_reg(&var_info.reg, &Size::DQ);
                    self.asm_fmt.fmt_ref_operand(&reg, &ty.to_bytes())
                } else {
                    self.asm_fmt.get_fmt_reg(&var_info.reg, forced_size)
                }
            }
            // それ以外(メモリ参照/即値/ポインタなど)は、サイズの
            // 上書きを行わず通常通り組み立てる
            _ => self.extract_operand_text(node_idx, in_self_ptr),
        }
    }

    /// inlineアセンブラのブロックを、実際のアセンブリテキストとして
    /// `self.asm_text`に書き出す。
    ///
    /// `lines`の各要素は`(プレースホルダー入りの行, オペランドのidx)`。
    /// `${var}`のような記述は、パーサー側(preproc.rs)の時点で既に
    /// `{0}`, `{1}`, ... の出現順のプレースホルダーへ変換済みで、
    /// 対応する式は`operands`(→ここでの`operand_ids`)に出現順で
    /// 積まれている。オペランドは変数・構造体のメンバー・ポインタの
    /// 参照/アドレス取得などを表すIRノードのidxで、通常の式と同じ
    /// `extract_operand_text`を使ってオペランドの文字列(レジスタ名や
    /// メモリ参照など)へ変換し、`{0}`, `{1}`, ... の出現順に埋め込む。
    pub(super) fn deploy_inline_asm(
        &mut self,
        name: &String,
        lines: &Vec<(String, Vec<usize>)>
    ) {
        if self
            .asm_fmt
            .inline_asm_list()
            .iter()
            .find(|v| v.as_str() == name.as_str())
            .is_some()
        {
            // === 1. インラインアセンブラ中で使われているレジスタを検出する ===
            let mut literal_regs: Vec<usize> = Vec::new();
            for (template, _) in lines.iter() {
                for reg in self.detect_literal_regs(template) {
                    if !literal_regs.contains(&reg) {
                        literal_regs.push(reg);
                    }
                }
            }

            // スタックへ退避した(=後で必ずpopして戻す必要がある)レジスタ
            let mut stacked_regs: Vec<usize> = Vec::new();

            // === 2. 検出したレジスタが使用中なら、退避方法を決めて処理する ===
            for reg in literal_regs.iter() {
                if let Some((var_name, size)) = self.var_using_reg(reg) {
                    if let Some(free_reg) = self.find_free_reg(&literal_regs) {
                        // --- 2-a. 空いているレジスタが見つかった場合 ---
                        // 値をそちらへ移し、以後はそのレジスタをこの
                        // 変数の正式な保持場所として扱う(恒久的な移動)
                        let src = self.asm_fmt.get_fmt_reg(reg, &size);
                        let dst = self.asm_fmt.get_fmt_reg(&free_reg, &size);
                        let mov_asm = self
                            .asm_fmt
                            .get_opcode_tmpl("mov")
                            .replace("{dst}", &dst)
                            .replace("{src1}", &src);
                        self.asm_text.push_str(&mov_asm);

                        self.update_value_reg(&var_name, &free_reg);
                    } else {
                        // --- 2-b. 空いているレジスタがない場合 ---
                        // インラインアセンブラの前後でこのレジスタの値を
                        // スタックに退避/復元する(一時的な退避)
                        let reg_name = self.asm_fmt.get_fmt_reg(reg, &Size::DQ);
                        let push_asm = self.asm_fmt.get_push(&reg_name);
                        self.asm_text.push_str(&push_asm);
                        stacked_regs.push(*reg);
                    }
                }
            }

            // === インラインアセンブラ本体を展開する ===
            for (template, operand_ids) in lines.iter() {
                let mut asm_line = template.clone();

                // AT&T記法の慣例により、最後のオペランドをdstとして扱う。
                // srcのレジスタサイズは、常にこのdstのサイズに合わせる
                // (dstが変数を参照していない場合は上書きしない)
                let dst_size = operand_ids
                    .last()
                    .and_then(|id| self.resolve_operand_var_size(id));
                let last_index = operand_ids.len().saturating_sub(1);

                for (index, operand_id) in operand_ids.iter().enumerate() {
                    let operand_text = match (index != last_index, &dst_size) {
                        (true, Some(size)) => {
                            self.extract_operand_text_sized(operand_id, false, size)
                        }
                        _ => self.extract_operand_text(operand_id, false),
                    };
                    // `{0}`, `{1}`, ... という数字のプレースホルダーを置換
                    // (`${var}`はパーサー側(preproc.rs)の時点で既に
                    //  `{index}`へ変換済みのため、ここでは数字の
                    //  プレースホルダーだけを見れば良い)
                    asm_line = asm_line.replace(&format!("{{{}}}", index), &operand_text);
                }

                asm_line.push('\n');
                self.asm_text.push_str(&asm_line);
            }

            // === スタックに退避したレジスタを、退避した時とは逆順に戻す ===
            for reg in stacked_regs.iter().rev() {
                let reg_name = self.asm_fmt.get_fmt_reg(reg, &Size::DQ);
                let pop_asm = self.asm_fmt.get_pop(&reg_name);
                self.asm_text.push_str(&pop_asm);
            }
        } else {
            panic!();
        }
    }
}
