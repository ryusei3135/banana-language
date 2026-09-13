///! 関数の中身を生成する関数は`src/gen/call_func.rs`にある
mod operand_txt;
mod struct_ir;
mod insert_fmt_reg;

use super::*;
use crate::asm_setting::{self, ValueFmt};
use crate::ir::types;

/// 演算結果を格納するレジスタの「適切なサイズ」が、テンプレートを
/// 組み立てる時点ではまだ決まらない演算子の一覧。
///
/// `*`(Mul)/`/`(Div)/`%`(Surplus)は`idiv`/`imul`を使うため、結果を
/// 格納するレジスタのサイズが、オペランドがメモリ上の値かどうかを
/// 見て初めて確定する(`format_expr_inst`内の`check_node_is_memory_value`
/// による判定より後)。そのため、テンプレートを組み立てる時点では
/// レジスタの「番号」だけを埋め込んでおき([`AsmEmitter::replace_insert_fmt_reg`]
/// を参照)、サイズが確定してから改めて実際のレジスタ名へ展開し直す
/// 必要がある。
pub(super) const DEFERRED_REG_FMT_OPS: &[inst::ExprKind] = &[
    inst::ExprKind::Mul,
    inst::ExprKind::Div,
    inst::ExprKind::Surplus,
];

#[derive(Debug, Clone)]
pub struct VarIndexInfo {
    /// `is_stack`が`false`の場合はレジスタ番号(従来通り)、
    /// `true`の場合は`%rbp`からのバイトオフセットを表す
    pub reg: usize,
    pub size: types::Size,
    pub index: usize,
    pub is_stack: bool,
}

impl VarIndexInfo {
    pub fn new(
        reg: &usize, 
        size: &types::Size, 
        index: &usize
    ) -> Self {
        Self {
            reg: *reg,
            size: size.clone(),
            index: *index,
            is_stack: false,
        }
    }

    /// `%rbp`からのオフセット(`offset`)に直接置かれている変数として登録する
    pub fn new_stack(
        offset: &usize, 
        size: &types::Size, 
        index: &usize
    ) -> Self {
        Self {
            reg: *offset,
            size: size.clone(),
            index: *index,
            is_stack: true,
        }
    }
}

/// 現在使用中のレジスタを管理する
#[derive(Debug, Clone, Default)]
pub struct UsedRegManager {
    used: Vec<usize>,
}

impl UsedRegManager {
    pub fn new() -> Self {
        Self { used: Vec::new() }
    }

    /// 指定したレジスタを使用中として記録する
    pub fn mark_used(&mut self, reg: &usize) {
        if !self.used.contains(reg) {
            self.used.push(*reg);
        }
    }

    /// 現在使用中のレジスタ番号を、使用され始めた順(昇順)に取得する
    pub fn used_regs(&self) -> Vec<usize> {
        let mut regs = self.used.clone();
        regs.sort();
        regs
    }

    /// 記録している使用中のレジスタの情報を全て消去する
    /// (関数一つ分のアセンブリ言語の生成が終わった際などに使う)
    pub fn clear(&mut self) {
        self.used.clear();
    }
}

pub struct AsmEmitter {
    pub(super) asm_text: String,
    pub(super) data_sec_text: String,
    pub(super) data_idx: usize,
    pub(super) reg_idx: usize,
    pub(super) expr_vars: Vec<String>,
    pub(super) reserved_label_name: Option<String>,

    pub(super) asm_fmt: mng_fmt::MngAsmFmt,

    pub(in crate::gen) curr_inst: Vec<inst::Inst>,
    // (親のid, 変数の名前)
    pub(super) data_map: Vec<(usize, String)>,
    pub(super) last_inst_idx: Vec<(usize, usize)>,
    pub(super) var_hash_map: HashMap<String, VarIndexInfo>,
    // 現在使用中のレジスタを管理する
    pub(super) used_reg: UsedRegManager,
    pub(super) stk_use_counter: usize,
}

impl AsmEmitter {
    pub fn new(
        asm_setting: asm_setting::AsmSetting, 
        asm_fmt: asm_setting::AsmFormat
    ) -> Self {
        let mut me = Self {
            asm_text: String::new(),
            data_sec_text: String::new(),
            data_idx: 0,
            reg_idx: 0,
            expr_vars: Vec::new(),
            reserved_label_name: None,

            asm_fmt: mng_fmt::MngAsmFmt::new(asm_setting, asm_fmt),
            curr_inst: Vec::new(),
            data_map: Vec::new(),
            last_inst_idx: Vec::new(),
            var_hash_map: HashMap::new(),
            used_reg: UsedRegManager::new(),
            stk_use_counter: 0,
        };
        me.data_sec_text
            .push_str(&me.asm_fmt.get_section_fmt("data"));
        me
    }

    pub fn to_asm_text(
        &mut self,
        func_tree: &mut def_tree::FuncTree,
        asm_fmt_name: &Option<String>,
        extern_funcs: &Vec<inst::Inst>,
        global_funcs: &Vec<String>,
    ) -> String {
        // `_start`が定義されているファイルの場合だけ、エントリー
        // ポイントとして`.global _start`を出力する
        // (#includeされるライブラリ用ファイルなど、`_start`を
        //  定義しないファイルにまでこれを出力してしまうと、
        //  存在しないシンボルを公開する不正なアセンブリになる)
        self.asm_text = if func_tree.func.contains_key("_start") {
            String::from(&self.asm_fmt.get_entry_point())
        } else {
            String::new()
        };

        self.gen_global_func_asm(&global_funcs);
        self.gen_extern_func_asm(&extern_funcs);

        // エントリーポイントを先頭に配置
        if let Some(ref mut meta_data) = func_tree
            .func
            .remove_entry("_start") 
        {
            self.build_func_process(meta_data, &asm_fmt_name);
        }

        for mut func_meta_data in func_tree.func.drain() {
            // == アセンブリ言語の生成 ==
            self.build_func_process(&mut func_meta_data, &asm_fmt_name);
            // == データの初期化 ==
            self.var_hash_map = HashMap::new();
            self.reg_idx = 0;
            self.used_reg.clear();
        }
        format!(
            "{}\n{}\n{}",
            self.data_sec_text,
            self.asm_fmt.get_section_fmt("text"),
            self.asm_text.replace("{space}", "  ")
        )
    }

    #[inline(always)]
    fn gen_global_func_asm(&mut self, global_funcs: &Vec<String>) {
        // 自身が公開する関数を生成
        for func_name in global_funcs.iter() {
            self.asm_text
                .push_str(self.asm_fmt.get_global_fmt(func_name).as_str());
        }
    }

    #[inline(always)]
    fn gen_extern_func_asm(
        &mut self, 
        extern_funcs: &Vec<inst::Inst>
    ) {
        for func in extern_funcs.iter() {
            if let inst::Inst::ExternFunc(name) = func {
                self.asm_text
                    .push_str(self.asm_fmt.get_extern_func(&name).as_str());
            } else {
                panic!();
            }
        }
    }

    /// 指定された名前の変数の型を返す
    pub fn get_var_ty(&self, var_name: &String) -> Size {
        self.var_hash_map
            .get(var_name)
            .expect(&format!("not found {}", var_name))
            .size
            .clone()
    }

    #[inline(always)]
    pub(super) fn insert_var_info(
        &mut self,
        name: &String, 
        var: VarIndexInfo) 
    {
        self.used_reg.mark_used(&var.reg);
        self.expr_vars.push(name.clone());
        self.var_hash_map.insert(name.clone(), var);
    }

    #[inline(always)]
    pub(super) fn update_value_info(
        &mut self, 
        name: &String, 
        index: &usize
    ) {
        self.var_hash_map
            .get_mut(name)
            .unwrap()
            .index = *index;
    }

    #[inline(always)]
    pub(super) fn update_value_reg(
        &mut self, 
        name: &String, 
        reg: &usize
    ) {
        self.used_reg.mark_used(reg);
        self.var_hash_map.get_mut(name).unwrap().reg = *reg;
    }

    /// 渡された情報を、設定したアセンブリ言語のフォーマット
    /// 通りに加工する。
    pub(super) fn format_line(
        &mut self,
        opcode: &str,
        dst: Option<&usize>,
        src1: &usize,
        src2: Option<&usize>,
        this_is_self: &SelfPtrInfo,
    ) -> String {
        let base_size: Size = self.check_node_is_mem_val(src1)
            .unwrap_or(Size::DQ);
        let mut formated = if let Some(struct_idx) = self.resolve_struct_idx(src1) {
            // 構造体の生成
            let mut txt = self.extract_operand_text(
                &struct_idx, 
                &this_is_self
            );

            let ret_line = if this_is_self.is_none() {
                let self_ptr_reg = self.asm_fmt.get_fmt_param::<String>(&0, Size::DQ);
                let line = self
                    .asm_fmt
                    .get_opcode_tmpl("address")
                    .replace("{dst}", &self.get_reg(dst, &Size::DQ))
                    .replace("{src1}", &self_ptr_reg);
                self.asm_fmt.fmt_mnemonic_resize("lea", &line, &Size::DQ)
            } else {
                self.asm_fmt
                    .get_opcode_tmpl(opcode)
                    .replace(
                        "{dst}", 
                        &self.get_reg(dst, &base_size)
                    )
                    .replace("{src1}", "%rbp")
            };
            txt.push_str(&ret_line);
            txt
        } else {
            let dst_size = if opcode == "address" {
                &Size::DQ
            } else {
                &base_size
            };
            self.asm_fmt
                .get_opcode_tmpl(opcode)
                .replace("{dst}", &self.get_reg(dst, dst_size))
                .replace("{src1}", &self.extract_operand_text(
                    src1, 
                    &this_is_self
                ))
        };

        if let Some(resized_asm) = self
            .gen_resize_mnemonic(&formated, opcode, &src1) 
        {
            formated = resized_asm;
        }

        if let Some(src2_id) = src2 {
            formated.replace(
                "{src2}", 
                &self.extract_operand_text(
                    src2_id, 
                    &this_is_self
                )
            )
        } else {
            formated
        }
    }

    #[inline(always)]
    fn gen_resize_mnemonic(
        &self, 
        formated: &String,
        opcode: &str,
        src1: &usize
    ) -> Option<String> {
        let mut resize = self.check_node_is_mem_val(src1).unwrap_or(Size::DQ);
        let mnemonic: &str = if opcode == "address" {
            "lea"
        } else if self.check_node_is_struct(&src1)
        {
            "mov"
        } else if let Some(size) = self
            .check_node_is_mem_val(&src1) 
        {
            resize = size;
            "mov"
        } else {
            return None;
        };
        Some(
            self.asm_fmt
                .fmt_mnemonic_resize(
                    mnemonic, 
                    formated, 
                    &resize
                )
        )
    }

    /// `idx`が(直接、あるいは`GetAddress`/`Pointer`でラップされた先に)
    /// `Inst::Struct`を指している場合、その`Inst::Struct`自身のidxを返す。
    /// `format_line`が構造体の生成を特別扱いする際、ラップされた
    /// ノードの内側までたどれるようにするためのヘルパー
    pub(super) fn resolve_struct_idx(
            &self, 
            idx: &usize
    ) -> Option<usize> {
        match &self.curr_inst[*idx] {
            inst::Inst::Struct { .. } => Some(*idx),
            inst::Inst::GetAddress(inner)
            | inst::Inst::Pointer(inner) => {
                self.resolve_struct_idx(inner)
            }
            _ => None,
        }
    }

    fn check_node_is_struct(&self, node_idx: &usize) -> bool {
        match &self.curr_inst[*node_idx] {
            inst::Inst::RefStruct { .. } => true,
            _ => false,
        }
    }

    /// 渡されたノードがスタック/静的領域の変数(`MemoryValue`)を
    /// 参照している場合、そのサイズを返す。
    /// これは`mov`命令に付けるサイズ接尾辞(`movl`など)を
    /// 決定するために使う。
    #[inline(always)]
    fn check_node_is_mem_val(
        &self, 
        node_idx: &usize
    ) -> Option<Size> {
        self.check_node_is_mem_val_inner(node_idx, false)
    }

    /// `check_node_is_mem_val`の実装本体
    /// ## 引数
    /// - is_deref
    ///     現在たどっている経路が`Inst::Pointer`(`*ptr`のような
    ///     参照先の読み書き)を経由しているかどうか。
    ///     `Inst::MemoryValue`にたどり着いたとき、これが`true`なら
    ///     ポインタが指す「参照先」の型のサイズ(`ty`)を、`false`なら
    ///     ポインタ変数「自身」の値のサイズ(常に8byte)を返す。
    fn check_node_is_mem_val_inner(
        &self,
        node_idx: &usize,
        is_deref: bool,
    ) -> Option<Size> {
        match &self.curr_inst[*node_idx] {
            inst::Inst::MemoryValue(
                inst::MemoryInst::Memory { size, .. }
            ) => match size {
                // `*ptr`のように参照先を読み書きする場合のみ、
                // 参照先の型`ty`のサイズを使う
                Size::Pointer { ty, .. } if is_deref => Some((**ty).clone()),
                // それ以外(ポインタ変数自身の値をそのまま読む場合)は、
                // ポインタの実際のサイズ(常に8byte)を使う
                Size::Pointer { .. } => Some(Size::DQ),
                other => Some(other.clone()),
            },
            inst::Inst::Pointer(inner) => {
                // ここから先は「参照先」をたどる経路になる
                self.check_node_is_mem_val_inner(inner, true)
            }
            inst::Inst::GetAddress(inner) => {
                // アドレスを求める経路では参照先をたどっているわけ
                // ではないので、`is_deref`はそのまま引き継ぐ
                self.check_node_is_mem_val_inner(inner, is_deref)
            }
            inst::Inst::InsertArr { name, .. } => {
                self.var_hash_map
                    .get(name)
                    .map(|var| var.size.clone())
            }
            _ => None,
        }
    }

    /// ## 引数
    /// - reg_idx これは必ずusizeで無ければいけない、
    fn get_reg(
        &self, 
        reg_idx: Option<&usize>, 
        size: &Size
    ) -> String {
        let num = if reg_idx.is_none() {
            self.reg_idx
        } else {
            *reg_idx.unwrap()
        };
        self.asm_fmt.get_fmt_reg(&num, &size)
    }

    pub(super) fn extract_operand_text(
        &mut self, 
        parent_id: &usize, 
        this_is_self: &Option<types::Size>
    ) -> String {
        match self.curr_inst[*parent_id].clone() {
            inst::Inst::Num { value, .. } => {
                self.asm_fmt.get_fmt_num(&value)
            }
            inst::Inst::GetPtr { size: _, stk } => {
                // スタック上に置かれた値そのもの(値が置かれているメモリ)
                // を指すオペランドを、`%rbp`からのオフセット`stk`を使って生成する
                self.asm_fmt.fmt_ref_operand(
                    &"%rbp".to_string(), 
                    &stk
                )
            }
            inst::Inst::Param(param) => {
                // `asm_emitter/operand_txt/`に記述
                self.param_ref(&param.name)
            }
            inst::Inst::AssignVar { ref name, .. } => {
                let var_info = self.var_hash_map
                    .get(&name.to_string())
                    .unwrap();
                let size =
                    if var_info
                        .size
                        .is_pointer()
                        .is_some() 
                    {
                        Size::DQ
                    } else {
                        var_info.size.clone()
                    };
                self.asm_fmt.get_fmt_reg(&var_info.reg, &size)
            }
            // 配列にアクセス
            inst::Inst::InsertArr { name, dst, index } => {
                // `asm_emitter/operand_txt/`に記述
                self.insert_arr_txt(&name, &dst, &index, this_is_self)
            }
            inst::Inst::Str { .. } => {
                // `asm_emitter/operand_txt/`に記述
                self.string_mem_ref(&parent_id)
            }
            inst::Inst::Mov { ref name, src, .. } => {
                // `asm_emitter/operand_txt/`に記述
                self.gen_mov_code(&name, &src)
            }
            inst::Inst::Block(name) => name.to_string(),
            inst::Inst::ExpectJmp(name) => name.to_string(),
            inst::Inst::Struct { mem, .. } => {
                self.emit_struct_ini_asm(
                    mem, 
                    // Noneの場合それはSelf
                    this_is_self.is_none()
                )
            }
            inst::Inst::MemoryValue(
                inst::MemoryInst::Memory { 
                    kind, 
                    size, 
                    .. 
                }) => 
            {
                // `asm_emitter/operand_txt/`に記述
                self.ref_mem_value_txt(&kind, &size, &parent_id)
            }
            inst::Inst::RefStruct { src, pos, .. } => {
                // `asm_emitter/operand_txt/`に記述
                self.ref_struct_txt(&src, &pos)
            }
            inst::Inst::GetAddress(index) => {
                self.extract_operand_text(&index.clone(), this_is_self)
            }
            // 配列リテラル自体を値として参照する場合
            // (例: 変数に束縛されずそのまま関数の引数などに使われる`{1,2,3}`)
            inst::Inst::InitArr(ids) => {
                // `asm_emitter/operand_txt/`に記述
                self.init_arr_txt::<false>(&ids, this_is_self);
                String::new()
            }
            inst::Inst::CallFunc(call_fn_info) => {
                // レジスタ0番、`%eax`など)をオペランドとして返す
                if call_fn_info.parent == crate::ir::IS_ASSIGN_EXPR {
                    let call_asm = self.emit_call_func(
                        &call_fn_info
                    );
                    self.asm_text.push_str(&call_asm);
                    self.asm_fmt.get_fmt_reg(&0, &Size::DQ)
                } else {
                    String::new()
                }
            }
            inst::Inst::Pointer(index) => {
                self.extract_operand_text(
                    &index.clone(),
                    this_is_self
                )
            }
            t => {
                if let Some(result) = self
                    .last_inst_idx
                    .iter()
                    .find(|i| &i.0 == parent_id) 
                {
                    // レジスタの文字列を取得
                    self.asm_fmt.get_fmt_reg(&result.1, &Size::DQ)
                } else {
                    panic!("{:?}", t);
                }
            }
        }
    }

    pub(super) fn format_expr_inst(
        &mut self, 
        expr: &inst::ExprInst
    ) -> String {
        let key = match expr.kind {
            inst::ExprKind::Add => "add",
            inst::ExprKind::Sub => "sub",
            inst::ExprKind::Mul => "mul",
            inst::ExprKind::Div => "div",
            inst::ExprKind::Surplus => "sur",
            inst::ExprKind::LessThen => "cmp_l",
            inst::ExprKind::GreaterThen => "cmp_g",
            inst::ExprKind::Equal => "cmp_e",
            inst::ExprKind::NotEq => "cmp_ne",
        };
        // ニーモニックのサイズ調整に使う、実際のニーモニックの文字列
        // (`cmp_l`/`cmp_g`はテンプレートを引くためのキーであって、
        //  実際にテンプレート中で使われるニーモニックは"cmp"なので、
        //  `fmt_mnemonic_resize`にはそちらを渡す必要がある)
        let mnemonic = match expr.kind {
            inst::ExprKind::Add => "add",
            inst::ExprKind::Sub => "sub",
            inst::ExprKind::Mul => "mul",
            inst::ExprKind::Div => "div",
            inst::ExprKind::Surplus => "sur",
            inst::ExprKind::LessThen
            | inst::ExprKind::GreaterThen
            | inst::ExprKind::NotEq
            | inst::ExprKind::Equal => "cmp",
        };

        let dst_text = if DEFERRED_REG_FMT_OPS.contains(&expr.kind) {
            Self::insert_fmt_reg_placeholder(&self.reg_idx)
        } else {
            self.get_reg(Some(&self.reg_idx), &Size::DQ)
        };

        let resolved_size = self
            .check_node_is_mem_val(&expr.ls)
            .or_else(|| self.check_node_is_mem_val(&expr.rs));
        let mut formated = self
            .asm_fmt
            .get_opcode_tmpl(key)
            .replace("{dst}", &dst_text)
            .replace("{src1}", &self.extract_operand_text(&expr.ls, &resolved_size))
            .replace("{src2}", &self.extract_operand_text(&expr.rs, &resolved_size))
            .to_string();

        if let Some(ref size) = resolved_size {
            formated = self.asm_fmt.fmt_mnemonic_resize("mov", &formated, size);
            formated = self.asm_fmt.fmt_mnemonic_resize(mnemonic, &formated, size);
        }

        if DEFERRED_REG_FMT_OPS.contains(&expr.kind) {
            let size = resolved_size.unwrap_or(Size::DQ);
            formated = self.replace_insert_fmt_reg(&formated, &size);
        }

        // サイズがSelfでない場合
        if self.reserved_label_name.is_some() 
            && formated.contains("{label}") 
        {
            let name = self.reserved_label_name
                .take()
                .unwrap();
            formated.replace("{label}", &name)
        } else {
            formated
        }
    }
}
