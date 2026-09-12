//! 出力するアセンブリ言語のフォーマットを管理する
//! アセンブリ言語のフォーマットを提供する:

use super::*;
use crate::asm_setting;
use crate::ir::types;
use std::any::{Any, TypeId};

/// アセンブリ言語のフォーマットをするAPIを提供
pub struct MngAsmFmt {
    param_fmt: Vec<usize>,
    reg_fmt: asm_setting::Reg,
    opcode_fmt: HashMap<String, asm_setting::OperandInfo>,
    pub(super) asm_setting: Option<asm_setting::AsmSetting>,
    fmt: asm_setting::AsmFormat,
}

impl MngAsmFmt {
    pub fn new(
        asm_setting: asm_setting::AsmSetting, 
        asm_fmt: asm_setting::AsmFormat
    ) -> Self {
        // === アセンブラのフォーマットの設定 ===
        Self {
            param_fmt: asm_fmt
                .args
                .fmt
                .get("linux")
                .unwrap()
                .clone(),
            reg_fmt: asm_fmt.reg.clone(),
            opcode_fmt: asm_fmt.op.clone(),
            asm_setting: Some(asm_setting),
            fmt: asm_fmt,
        }
    }

    /// 外部に定義されている物のフォーマット
    pub fn get_extern_func(&self, name: &String) -> String {
        self.fmt.func.extern_def.replace("{name}", name)
    }

    /// 設定された、関数の引数のレジスタを返す
    /// ## Rについて
    /// - Stringの場合はレジスタが返される
    /// - usizeの場合はレジスタの番号が返される
    /// ## 引数
    /// - param_idx = 引数の場所
    pub(in crate::gen) fn get_fmt_param<R: 'static>(
        &self, 
        param_idx: &usize, 
        size: Size
    ) -> R {
        if TypeId::of::<R>() == TypeId::of::<String>()
        {
            let result: Box<dyn Any> = Box::new(
                self.get_fmt_reg(
                        &self.param_fmt[*param_idx], 
                        &size
                    )
                    .to_string(),
            );
            result.downcast::<R>()
                .ok()
                .map(|b| *b)
                .unwrap()
        } else if TypeId::of::<R>() == TypeId::of::<usize>() {
            let result: Box<dyn Any> = Box::new(self.param_fmt[*param_idx]);
            result.downcast::<R>()
                .ok()
                .map(|b| *b)
                .unwrap()
        } else {
            panic!("この型は無効です,")
        }
    }

    pub fn fmt_ref_operand(
        &self, 
        reg: &String, 
        size: &usize
    ) -> String {
        self.fmt
            .fmt
            .ref_stack
            .replace("{src}", reg.as_str())
            .replace("{size}", size.to_string().as_str())
    }

    pub fn func_frame_fmt(&self) -> String {
        self.fmt.fmt.frame.to_string()
    }

    pub fn func_frame_end(&self) -> String {
        self.fmt.fmt.frame_end.to_string()
    }

    /// 予約されていた、スタックのサイズ分
    pub fn gen_stack_frame(&self, size: usize) -> String {
        // 8バイト境界に切り上げてアライメントする
        // (例: size=1..8 -> 8, size=9..16 -> 16)
        let remainder = size % 8;
        let alignment_size = if remainder == 0 {
            size
        } else {
            size + (8 - remainder)
        };
        self.fmt
            .fmt
            .data
            .head
            // 確保するスタックのサイズ
            .replace("{size}", &alignment_size.to_string())
    }

    /// 構造体のメンバーをフォーマット
    /// ## 引数
    /// - value = 代入する値
    /// - size = このメンバーの型のサイズ(movのニーモニックの決定にのみ使う)
    /// - offset = `%rbp`からのこのメンバーの、確定済みのオフセット
    ///   (呼び出し側で、これまでの`stk_use_counter`とこのメンバー分の
    ///   サイズを合計した値を渡す。ここで更にサイズを足してはいけない)
    #[inline(always)]
    pub fn get_fmt_struct_member(
        &self,
        value: String,
        size: &types::Size,
        offset: &usize,
    ) -> String {
        let fmted = self
            .fmt
            .fmt
            .data
            .fmt
            .replace("{dst}", value.as_str())
            .replace("{size}", offset.to_string().as_str());
        let s_fmt = match &size {
            types::Size::DB => crate::mov_size_fmt!(self, db),
            types::Size::DW => crate::mov_size_fmt!(self, dw),
            types::Size::DD => crate::mov_size_fmt!(self, dd),
            types::Size::DQ => crate::mov_size_fmt!(self, dq),
            types::Size::Array { size, .. } => {
                return self.get_fmt_struct_member(value, &size, &offset)
            }
            _ => panic!(),
        };
        fmted.replace("mov", &format!("mov{}", s_fmt))
    }

    /// ニーモニックのサイズを調整
    pub fn fmt_mnemonic_resize(
        &self,
        mnemonic: &str,
        value: &String,
        size: &types::Size,
    ) -> String {
        if let types::Size::Pointer { ty, .. } = size {
            return self.fmt_mnemonic_resize(mnemonic, value, ty);
        }

        let s_fmt = match &size {
            types::Size::DB => crate::mov_size_fmt!(self, db),
            types::Size::DW => crate::mov_size_fmt!(self, dw),
            types::Size::DD => crate::mov_size_fmt!(self, dd),
            types::Size::DQ => crate::mov_size_fmt!(self, dq),
            types::Size::Pointer { .. } => unreachable!(),
            types::Size::Array { size, .. } => {
                return self.fmt_mnemonic_resize(&mnemonic, &value, &size);
            }
            t => panic!("{:?}", t),
        };
        value.replace(mnemonic, &format!("{}{}", mnemonic, s_fmt))
    }

    pub fn get_push(&self, reg: &String) -> String {
        self.get_opcode_tmpl(&"push".to_string())
            .replace("{dst}", reg)
    }

    pub fn get_pop(&self, reg: &String) -> String {
        self.get_opcode_tmpl(&"pop".to_string())
            .replace("{dst}", reg)
    }

    pub fn get_str_fmt(
        &self, 
        value: &String, 
        label: &String
    ) -> String {
        self.fmt
            .fmt
            .string
            .replace("{}", value)
            .replace("{name}", &label)
    }

    pub fn get_static_num_fmt(
        &self,
        value: &String,
        label: &String,
        _size: &types::Size,
    ) -> String {
        format!(".align 4\n{}: .long {}\n", label, value.replace("$", ""))
    }

    #[inline(always)]
    pub fn get_global_fmt(&self, name: &String) -> String {
        self.fmt.fmt.global.replace("{name}", name)
    }

    /// 静的領域の変数に%ripをつけて返す
    #[inline(always)]
    pub fn fmt_static_var_rip(&self, name: &String) -> String {
        self.fmt.fmt.static_var.replace("{name}", name)
    }

    /// エントリーポイントを作成
    pub fn get_entry_point(&self) -> String {
        self.fmt
            .fmt
            .global
            .replace("{name}", &self.asm_setting.as_ref().unwrap().entry)
    }

    /// アセンブリ言語のセクションを定義するフォーマット
    pub fn get_section_fmt(&self, section: &str) -> String {
        self.fmt.section.replace("{name}", section)
    }

    /// オペコードのフォーマット
    pub fn get_opcode_tmpl(&self, key: &str) -> String {
        self.opcode_fmt
            .get(key)
            // 渡されたキーがない
            .expect(&format!("not found key {}", key))
            .template
            .clone()
    }

    /// 数字のフォーマット
    #[inline(always)]
    pub fn get_fmt_num(&self, value: &String) -> String {
        self.fmt.fmt.num.replace("{}", value).to_string()
    }

    #[inline(always)]
    pub fn get_call_func_fmt(&self, func_name: &String) -> String {
        self.fmt.func.call.replace("{name}", func_name)
    }

    pub fn get_fmt_reg(
        &self, 
        reg_num: &usize, 
        size: &Size
    ) -> String {
        let reg = match &size {
            Size::DB => &self.reg_fmt.db,
            Size::DW => &self.reg_fmt.dw,
            Size::DD => &self.reg_fmt.dd,
            Size::DQ => &self.reg_fmt.dq,
            Size::Pointer{..} => &self.reg_fmt.dq,
            _ => panic!(),
        };
        self.fmt
            .fmt
            .reg
            .replace("{}", reg[*reg_num].as_str())
            .to_string()
    }

    /// レジスタ文字列からレジスタのサイズを取得する
    pub fn get_reg_size(
        &self, 
        reg_name: &str
    ) -> Option<Size> {
        for (size, registers) in [
            (Size::DB, &self.reg_fmt.db),
            (Size::DW, &self.reg_fmt.dw),
            (Size::DD, &self.reg_fmt.dd),
            (Size::DQ, &self.reg_fmt.dq),
        ] {
            for register in registers {
                let formatted = self.fmt.fmt.reg.replace("{}", register);
                if formatted == reg_name {
                    return Some(size);
                }
            }
        }
        None
    }

    /// 渡された文字列がレジスタのオペランド(`%eax`など)である場合、
    /// 「同じレジスタ」の`size`版のオペランド文字列(`%rax`など)に
    /// 変換して返す。
    ///
    /// `operand`がレジスタでない場合(即値やメモリ参照`-8(%rbp)`など)
    /// は、`get_reg_size`が`None`を返すのでそのまま変更せず返す。
    pub fn resize_reg_operand(
        &self,
        operand: &str,
        size: &Size,
    ) -> String {
        for (_, registers) in [
            (Size::DB, &self.reg_fmt.db),
            (Size::DW, &self.reg_fmt.dw),
            (Size::DD, &self.reg_fmt.dd),
            (Size::DQ, &self.reg_fmt.dq),
        ] {
            for (reg_num, register) in registers.iter().enumerate() {
                let formatted = self.fmt.fmt.reg.replace("{}", register);
                if formatted == operand {
                    return self.get_fmt_reg(&reg_num, size);
                }
            }
        }
        operand.to_string()
    }

    /// 確保されているレジスタの本数
    #[inline(always)]
    pub fn reg_count(&self) -> usize {
        self.reg_fmt.dq.len()
    }

    /// `(レジスタ番号, レジスタ名)`のペアを、確保されている
    /// 全レジスタ・全サイズ分列挙する。
    pub fn all_reg_names(&self) -> Vec<(usize, String)> {
        let mut result = Vec::new();
        for reg_idx in 0..self.reg_count() {
            for size in [Size::DQ, Size::DD, Size::DW, Size::DB] {
                result.push((reg_idx, self.get_fmt_reg(&reg_idx, &size)));
            }
        }
        result
    }

    #[inline(always)]
    pub fn inline_asm_list(&self) -> Vec<String> {
        self.asm_setting.as_ref().unwrap().get_inline_asm_list()
    }
}