use super::*;

impl AsmEmitter {
    /// メモリに値を書き込むアセンブリ言語を生成
    pub(super) fn write_mem(
        &mut self,
        name: &String,
        dst: &usize,
        value: &usize,
        this_is_self: &Option<types::Size>,
    ) {
        // 書き込み先のメモリのオペランド
        let dst_operand = self.extract_operand_text(dst, &this_is_self);
        let dst_operand = if matches!(self.curr_inst[*dst], inst::Inst::Pointer(..)) {
            format!("({})", dst_operand)
        } else {
            dst_operand
        };
        // 書き込む値のオペランド
        let value_operand = self.extract_operand_text(value, &this_is_self);

        let mut text = self
            .asm_fmt
            .get_opcode_tmpl("mov")
            .replace("{dst}", &dst_operand)
            .replace("{src1}", &value_operand);

        // ニーモニックのサイズ調整に使う型。
        //
        // `[ptr] = 20`/`[arr 0] = 10`のケースでは、`name`(ポインタ/
        // 配列の変数名)の型(`Pointer{ty}`/`Array{size,..}`)がそのまま
        // 書き込む値のサイズを表しているため、`get_var_ty(&name)`で
        // 問題なかった。
        let mnemonic_size = match &self.curr_inst[*dst] {
            inst::Inst::RefStruct { .. } => match &self.curr_inst[*value] {
                inst::Inst::Num { size, .. } => size.clone(),
                _ => self.get_var_ty(&name),
            },
            _ => self.get_var_ty(&name),
        };

        text = self.asm_fmt.fmt_mnemonic_resize("mov", &text, &mnemonic_size);
        self.asm_text.push_str(&text);
    }

    pub(super) fn assign_val_ty_is_ptr(
        &mut self,
        current_reg: usize,
        value: &usize,
        this_is_self: &Option<types::Size>,
    ) -> String {
        // ポインタ型の変数へ数値リテラル(`ptr = 0`のようなNULL代入)を
        // 再代入する場合は、アドレスを求める`lea`ではなく、ポインタの
        // サイズ(64bit)に合わせた`movq`でそのまま即値を書き込む
        if matches!(self.curr_inst[*value], inst::Inst::Num { .. }) {
            let dst_reg = self.asm_fmt.get_fmt_reg(&current_reg, &Size::DQ);
            let value_operand = self.extract_operand_text(
                value, 
                &this_is_self
            );
            let text = self
                .asm_fmt
                .get_opcode_tmpl("mov")
                .replace("{dst}", &dst_reg)
                .replace("{src1}", &value_operand);
            return self.asm_fmt.fmt_mnemonic_resize("mov", &text, &Size::DQ);
        }

        let dst_reg = self.asm_fmt.get_fmt_reg(&current_reg, &Size::DQ);

        let ptr_operand = match &self.curr_inst[*value] {
            inst::Inst::GetPtr { size, .. } => {
                self.asm_fmt.fmt_ref_operand(&"rbp".to_string(), &size)
            }
            _ => self.extract_operand_text(value, &this_is_self),
        };

        self.asm_fmt
            .fmt_mnemonic_resize(
            "address",
            &self.asm_fmt
                .get_opcode_tmpl("address")
                .replace("{dst}", &dst_reg)
                .replace("{src1}", &ptr_operand),
                &Size::DQ
            )
    }

    pub(super) fn assign_val_is_not_ptr(
        &mut self,
        current_reg: usize,
        value: &usize,
        this_is_self: &Option<types::Size>,
    ) -> String {
        let mnemonic = if self.curr_inst[*value]
            .is_pointer() 
        {
            "address"
        } else {
            "mov"
        };

        self.format_line(
            mnemonic, 
            Some(&current_reg), 
            &value, 
            None, 
            &this_is_self
        )
    }
}
