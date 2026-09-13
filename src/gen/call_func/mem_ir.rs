use super::*;
use crate::ir::types;

impl AsmEmitter {
    pub(super) fn mov_value_ir(
        &mut self,
        size: &types::Size,
        dst: &usize,
        src: &usize,
        name: &Option<String>,
        this_is_self: &SelfPtrInfo,
    ) {
        // 経由の間接参照になってしまっていた。
        if let types::Size::Struct(_) = size {
            // 呼び出し自体(`lea`+`call`)は副作用として`self.asm_text`へ
            // 積まれる。戻り値のオペランド文字列自体は構造体には
            // 使えないので捨てる
            let _ = self.extract_operand_text(
                src, 
                &this_is_self
            );
            if self.struct_mem(
                    name, 
                    size, 
                    src, 
                    dst
                ).is_none() 
            {
                return ();
            }

            let inst::Inst::CallFunc(meta_data) = 
                self.curr_inst[*src].clone() else 
            {
                panic!("構造体を返す初期化式はコンストラクタ呼び出しである必要があります: {:?}", self.curr_inst[*src]);
            };
            let self_arg_idx = *meta_data
                .params
                .get(0)
                .expect("構造体を返す関数は暗黙のselfポインタ引数を持つ必要があります");
            let inst::Inst::GetAddress(mem_idx) = self.curr_inst[self_arg_idx].clone() else {
                panic!("システムエラー: 暗黙のselfポインタ引数がGetAddressではありません");
            };
            let inst::Inst::GetPtr { stk, .. } = self.curr_inst[mem_idx].clone() else {
                panic!("システムエラー: 暗黙のselfポインタ引数の参照先がGetPtrではありません");
            };

            if let Some(var_name) = name {
                self.insert_var_info(
                    var_name,
                    asm_emitter::VarIndexInfo::new_stack(
                        &stk, 
                        &size, 
                        dst
                    ),
                );
            }
            return;
        }

        if size.is_pointer().is_none() 
            && self.data_map
                .iter()
                .find(|v| &v.0 == src)
                .is_some()
        {
            // 子のノードがstatic領域の値で、かつ宣言先の型がポインタで
            // ない場合のみ、変数名だけを登録する(この場合は変数の
            // 実体が静的領域そのものであり、レジスタへ値をロード
            // する必要がないため)。
            self.insert_var_info(
                &name.as_ref().unwrap(),
                asm_emitter::VarIndexInfo::new(
                    &self.reg_idx, 
                    &size, 
                    dst
                ),
            );
        } else {
            self.reg_idx += 1;
            // レジスタに置く変数
            let reg = self.reg_idx.clone();
            // このレジスタを使用中として記録する
            self.used_reg.mark_used(&reg);
            // ポインタ型の変数へ数値リテラル(`ptr: int* = 0`の
            // ような`0`=NULL初期化など)を代入する場合は、
            // アドレスを求める`lea`(="address")命令ではなく、
            // ポインタのサイズ(64bit)に合わせた`movq`で
            // そのまま即値をレジスタへ書き込む
            let is_literal_num = matches!(self.curr_inst[*src], inst::Inst::Num { .. });

            let formated = if size
                .is_pointer()
                .is_some() && is_literal_num 
            {
                let dst_reg = self.asm_fmt.get_fmt_reg(&reg, &Size::DQ);
                let value_operand = self.extract_operand_text(&src, &this_is_self);
                let text = self
                    .asm_fmt
                    .get_opcode_tmpl("mov")
                    .replace("{dst}", &dst_reg)
                    .replace("{src1}", &value_operand);
                self.asm_fmt.fmt_mnemonic_resize("mov", &text, &Size::DQ)
            } else {
                // メモリのポインタか、値かで、ニーモニックが変わる
                let mnemonic = if size.is_pointer().is_some() {
                    // ポインタの場合
                    "address"
                } else {
                    // 普通の場合
                    "mov"
                };
                self.format_line(mnemonic, Some(&reg), &src, None, this_is_self)
            };

            self.asm_text.push_str(&formated);

            if let Some(var_name) = name {
                self.insert_var_info(
                    &var_name,
                    asm_emitter::VarIndexInfo::new(&self.reg_idx, &size, dst),
                );
                let current_reg = self.reg_idx;

                if self
                    .expr_vars
                    .iter()
                    .find(|v| v.as_str() == var_name.as_str())
                    .is_some()
                {
                    self.update_value_reg(&var_name, &current_reg);
                }
            }
        }
    }

    pub(super) fn mem_val_ir(
        &mut self, 
        mem_value: &inst::MemoryInst, 
        this_is_self: &bool,
    ) {
        match mem_value {
            inst::MemoryInst::Memory {
                name,
                size,
                src,
                kind,
                dst,
            } => {
                let dst_size: SelfPtrInfo = size.wrap_dst_size();

                if kind == &inst::MemoryKind::Static {
                    self.is_static_var(
                        src, 
                        &dst, 
                        name, 
                        &size.wrap_dst_size()
                    );
                } else {
                    let base = match &self.curr_inst[*dst] {
                        inst::Inst::Pointer(..)
                        | inst::Inst::Param(..)
                        | inst::Inst::GetPtr { .. } => {
                            self.extract_operand_text(
                                &dst, 
                                &dst_size
                            )
                        }
                        _ => "%rbp".to_string(),
                    };

                    let mut txt = String::new();
                    for idx in src.iter() {
                        let value = self.extract_operand_text(&idx, &dst_size);
                        // スタックの場所を更新
                        // (この変数のオフセットは、これまで使用した
                        //  スタックのサイズ`stk_use_counter`に、
                        //  この変数のサイズを足したもの)
                        self.stk_use_counter += size.to_bytes();
                        let s = &self.asm_fmt.fmt_ref_operand(&base, &self.stk_use_counter);

                        let mov_line = self
                            .asm_fmt
                            .get_opcode_tmpl("mov")
                            .replace("{dst}", &s)
                            .replace("{src1}", value.as_str());
                        txt.push_str(
                            self.asm_fmt
                                .fmt_mnemonic_resize("mov", &mov_line, &size)
                                .as_str(),
                        );
                    }
                    self.insert_var_info(
                        &name,
                        asm_emitter::VarIndexInfo::new(
                            &self.reg_idx, 
                            &size, 
                            dst
                        ),
                    );
                    self.asm_text.push_str(txt.as_str());
                }
            }
            _ => panic!(),
        }
    }

    fn is_static_var(
        &mut self,
        src: &Vec<usize>,
        dst: &usize,
        name: &String,
        this_is_self: &SelfPtrInfo,
    ) {
        println!("src/gen/call_func/MemoryValue");
        let val = self.extract_operand_text(
            &src.last().unwrap(), 
            &this_is_self
        );
        let label_name = format!("M{}", self.data_idx.to_string());
        let fmt_data = self.asm_fmt
            .get_static_num_fmt(
                &val, 
                &label_name, 
                this_is_self.as_ref().unwrap()
            );
        self.data_sec_text.push_str(&fmt_data);
        self.data_map.push((*dst, label_name));
        self.data_idx += 1;
        // 子のノードがstaticりょいきの値なので、変数名だけ登録する
        self.insert_var_info(
            &name,
            asm_emitter::VarIndexInfo::new(
                &self.reg_idx, 
                this_is_self.as_ref().unwrap(), 
                dst
            ),
        );
    }

    fn struct_mem(
        &mut self, 
        name: &Option<String>, 
        size: &types::Size,
        src: &usize,
        dst: &usize,
    ) -> Option<()> {
        if let inst::Inst::Struct { 
            mem, 
            is_self, 
            .. 
        } = self.curr_inst[*src].clone() {
            // `emit_struct_ini_asm`はメンバーを書き込みながら
            // `self.stk_use_counter`を進めていくため、呼び出し後
            // では構造体自身の先頭オフセット(`%rbp`から見た位置)が
            // 分からなくなってしまう。呼び出し前の値を控えておき、
            // それをこの変数の実体の位置として登録する
            let struct_stk_offset = self.stk_use_counter;
            let ini_asm = 
                self.emit_struct_ini_asm(mem, is_self);
            self.asm_text.push_str(
                ini_asm.as_str()
            );
            if let Some(var_name) = name {
                self.insert_var_info(
                    var_name,
                    asm_emitter::VarIndexInfo::new_stack(
                        &struct_stk_offset,
                        &size,
                        dst,
                    ),
                );
            }
            return None;
        }
        Some(())
    }
}
