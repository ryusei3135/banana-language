//! `extract_operand_text`からのみ呼び出すAPI

use super::*;

impl AsmEmitter {
    pub(super) fn param_ref(
        &mut self, 
        param_name: &String
    ) -> String {
        let var_info = self.var_hash_map
            .get(&param_name.to_string())
            .unwrap();

        if let Some(ty) = var_info.size.is_pointer() {
            // ポインタ型はアドレス(常に8byte)を保持するため、
            // 64bitレジスタ(`%rcx`など)を経由してメモリを参照する
            let reg = self.asm_fmt.get_fmt_reg(
                &var_info.reg, 
                &Size::DQ
            );
            self.asm_fmt.fmt_ref_operand(&reg, &ty.to_bytes())
        } else {
            self.asm_fmt.get_fmt_reg(
                &var_info.reg, 
                &var_info.size
            )
        }
    }

    pub(super) fn string_mem_ref(
        &mut self, 
        parent_id: &usize
    ) -> String {
        let label = self
            .data_map
            .iter()
            .find(|v| &v.0 == parent_id)
            .unwrap()
            .1
            .clone();
        self.asm_fmt.fmt_static_var_rip(&label)
    }

    /// 配列に値を代入するコードを生成
    pub(super) fn insert_arr_txt(
        &mut self,
        name: &String,
        dst: &usize,
        index: &usize,
        this_is_self: &SelfPtrInfo,
    ) -> String {
        let index_value = match &self.curr_inst[*index] {
            inst::Inst::Num { value, .. } => value
                .parse::<usize>()
                .expect("配列の添字は数字である必要があります"),
            t => panic!("配列の添字には数字のノードが必要です: {:?}", t),
        };
        let pos = {
            let var_info = self.var_hash_map
                .get(&name.to_string())
                .unwrap();
            if let Some(pointee) = var_info.size.is_pointer() {
                pointee.to_bytes() * index_value
            } else {
                let size = var_info.size.to_bytes();
                size * index_value + size
            }
        };

        let base = self.extract_operand_text(
            &dst, 
            &this_is_self
        );
        self.asm_fmt.fmt_ref_operand(&base, &pos)
    }

    /// 構造体を参照するコードを作成
    pub(super) fn ref_struct_txt(
        &mut self, 
        src: &str, 
        size: &usize
    ) -> String {
        let var_info = self
            .var_hash_map
            .get(&src.to_string())
            .expect(&src);

        if var_info.is_stack {
            // `src`(構造体変数自身)が、ポインタをレジスタに持つのでは
            // なく`%rbp`相対のメモリに直接置かれている場合
            // (`a: Name = Name::new()`のように構造体を直接ローカル
            //  変数として初期化した場合など)。
            // このとき`var_info.reg`には`%rbp`からの構造体先頭の
            // オフセットが入っているので、そこにメンバーのオフセット
            // (`size`)を足した位置を直接`%rbp`相対で参照する
            // (レジスタを経由した間接参照`(%rcx)`にはしない)
            let offset = var_info.reg + size;
            self.asm_fmt.fmt_ref_operand(&"%rbp".to_string(), &offset)
        } else {
            self.asm_fmt.fmt_ref_operand(
                &self.asm_fmt.get_fmt_reg(&var_info.reg, &Size::DQ),
                &size,
            )
        }
    }

    pub(super) fn gen_mov_code(
        &mut self, 
        name: &Option<String>, 
        src: &usize
    ) -> String {
        let Some(var_name) = name else {
            panic!()
        };

        let var = self
            .var_hash_map
            .get(&*var_name)
            .unwrap_or_else(|| panic!("this var is not found -> {}", var_name));

        if var.is_stack {
            return self
                .asm_fmt
                .fmt_ref_operand(
                    &"%rbp".to_string(), 
                    &var.reg
                );
        }
        let (reg_num, is_ptr, var_size) =
            (var.reg, var.size.is_pointer().is_some(), var.size.clone());
        let size = if is_ptr { Size::DQ } else { var_size };
        if !is_ptr {
            if let Some(static_var) = self.data_map
                .iter()
                .find(|v| &v.0 == src) 
            {
                // static領域の変数を返す:
                return static_var.1.clone();
            }
        }

        self.reg_idx = reg_num.clone();
        self.asm_fmt.get_fmt_reg(&reg_num, &size)
    }

    /// メモリを参照するコードを生成する
    pub(super) fn ref_mem_value_txt(
        &mut self,
        kind: &inst::MemoryKind,
        size: &Size,
        parent_id: &usize,
    ) -> String {
        if matches!(kind, inst::MemoryKind::Static) {
            // 静的領域の変数: データセクションに置いたラベルを参照する
            let name = self
                .data_map
                .iter()
                .find(|v| &v.0 == parent_id)
                .expect("static var label not found")
                .1
                .clone();
            self.asm_fmt.fmt_static_var_rip(&name)
        } else {
            // スタック領域の変数: %rbpからのオフセットを参照する
            self.asm_fmt
                .fmt_ref_operand(&"%rbp".to_string(), &size.to_bytes())
        }
    }

    /// `Inst::InitArr`(変数に束縛されていない配列リテラル、
    /// 例: 関数の引数にそのまま渡された`{1,2,3}`)を
    /// スタック上に展開し、その先頭要素を指すオペランドを返す
    ///
    /// ## 引数
    /// - ids: 配列の各要素の値を持つノード(`Inst::Num`など)のid
    pub(super) fn init_arr_txt<const RET_IS_ASM: bool>(
        &mut self,
        ids: &Vec<usize>,
        this_is_self: &SelfPtrInfo,
    ) -> String {
        // 代入する先が構造体などの自身のポインタの場合、引数のレジスタにする
        let assign_reg = if this_is_self.is_none() {
            self.asm_fmt.get_fmt_param::<String>(&0, Size::DQ)
        } else {
            "%rbp".to_string()
        };

        let Some(first_id) = ids.first() else {
            panic!("空の配列リテラルはサポートされていません");
        };

        // 配列の要素のサイズは先頭の要素から求める
        // (配列の要素は全て同じ型/サイズであることが前提)
        let size = match &self.curr_inst[*first_id] {
            inst::Inst::Num { size, .. } => size.clone(),
            t => panic!("配列の要素には数字のノードが必要です: {:?}", t),
        };

        let mut txt = String::new();
        let mut head_offset = None;

        for id in ids.iter() {
            let value = self.extract_operand_text(
                id, 
                &this_is_self
            );

            // スタックの場所を更新
            // (この要素のオフセットは、これまで使用したスタックのサイズ
            //  `stk_use_counter`に、この要素のサイズを足したもの)
            self.stk_use_counter += size.to_bytes();
            if head_offset.is_none() {
                head_offset = Some(self.stk_use_counter);
            }

            let dst = self
                .asm_fmt
                .fmt_ref_operand(&assign_reg, &self.stk_use_counter);

            let mov_line = self
                .asm_fmt
                .get_opcode_tmpl("mov")
                .replace("{dst}", &dst)
                .replace("{src1}", value.as_str());

            txt.push_str(
                self.asm_fmt
                    .fmt_mnemonic_resize("mov", &mov_line, &size)
                    .as_str(),
            );
        }

        if RET_IS_ASM {
            return txt;
        }
        // サイズがSelfポインタでない場合
        if this_is_self.is_some() {
            self.asm_text.push_str(txt.as_str());
        }
        self.asm_fmt
            .fmt_ref_operand(
                &assign_reg, 
                &head_offset.unwrap()
            )
    }
}
