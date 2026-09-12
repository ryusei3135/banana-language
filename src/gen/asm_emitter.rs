///! 関数の中身を生成する関数は`src/gen/call_func.rs`にある
mod operand_txt;
mod struct_ir;
mod insert_fmt_reg;

use super::*;
use crate::asm_setting;
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
    pub fn new(reg: &usize, size: &types::Size, index: &usize) -> Self {
        Self {
            reg: *reg,
            size: size.clone(),
            index: *index,
            is_stack: false,
        }
    }

    /// `%rbp`からのオフセット(`offset`)に直接置かれている変数として登録する
    pub fn new_stack(offset: &usize, size: &types::Size, index: &usize) -> Self {
        Self {
            reg: *offset,
            size: size.clone(),
            index: *index,
            is_stack: true,
        }
    }
}

/// 現在使用中のレジスタを管理する
///
/// レジスタは`reg_idx`によって使い切り(一度使ったレジスタ番号を
/// 再利用しない)方式で割り当てられていくため、ここでは「これまでに
/// 使用された(かつまだ解放されていない)レジスタ番号」を記録しておく。
///
/// インラインアセンブラを展開する際、ここに記録されているレジスタを
/// 全て`push`しておき、展開が終わったら`pop`することで、インライン
/// アセンブラの中身によって使用中のレジスタの値が破壊されるのを防ぐ。
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
        if let Some(ref mut meta_data) = func_tree.func.remove_entry("_start") {
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
    fn gen_extern_func_asm(&mut self, extern_funcs: &Vec<inst::Inst>) {
        for func in extern_funcs.iter() {
            if let inst::Inst::ExternFunc(name) = func {
                self.asm_text
                    .push_str(self.asm_fmt.get_extern_func(&name).as_str());
            } else {
                panic!();
            }
        }
    }

    pub fn get_var_ty(&self, name: &String) -> Size {
        self.var_hash_map
            .get(name)
            .expect(&format!("not found {}", name))
            .size
            .clone()
    }

    #[inline(always)]
    pub(super) fn insert_var_info(&mut self, name: &String, var: VarIndexInfo) {
        self.used_reg.mark_used(&var.reg);
        self.expr_vars.push(name.clone());
        self.var_hash_map.insert(name.clone(), var);
    }

    #[inline(always)]
    pub(super) fn update_value_info(&mut self, name: &String, index: &usize) {
        self.var_hash_map.get_mut(name).unwrap().index = *index;
    }

    #[inline(always)]
    pub(super) fn update_value_reg(&mut self, name: &String, reg: &usize) {
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
        this_is_self: bool,
    ) -> String {
        let mut formated = if let Some(struct_idx) = self.resolve_struct_idx(src1) {
            // 構造体の生成
            let mut txt = self.extract_operand_text(&struct_idx, this_is_self);

            // 構造体を`ret`する行は、構造体の「値」ではなく、それが
            // 置かれているメモリの「アドレス」を戻り値のレジスタへ
            // 返す必要がある。
            // - メゾット内(`this_is_self`)の場合、構造体は`self`
            //   ポインタ(呼び出し規約上の第一引数のレジスタ、
            //   `%rdi`など)が指す先に直接書き込まれているため、
            //   返すアドレスも`%rbp`ではなくこの`self`ポインタで
            //   なければならない。またアドレスを求める処理なので、
            //   `mov`ではなく`lea`(テンプレート上のキーは`address`)
            //   を使う必要がある。
            //   以前はここが常に`opcode`(呼び出し元から渡された
            //   `"mov"`)と`%rbp`決め打ちだったため、`self`のポインタ
            //   を無視した`mov %rbp, %rax`という誤ったコードが
            //   生成されてしまっていた。
            let ret_line = if this_is_self {
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
                    .replace("{dst}", &self.get_reg(dst, &Size::DQ))
                    .replace("{src1}", "%rbp")
            };
            txt.push_str(&ret_line);
            txt
        } else {
            // `address`(=`lea`)はアドレス(ポインタ)を求める命令なので、
            // 常にポインタサイズ(8byte = %rcxなどの64bitレジスタ)の
            // レジスタを使わなければならない。
            // ここを他のケースと同様に`Size::DD`(32bit)のままにすると、
            // 後段で`fmt_mnemonic_resize`によってニーモニックだけが
            // "leaq"のように64bit用に調整される一方でレジスタ表記は
            // 32bit(`%ecx`など)のままになってしまい、ニーモニックと
            // レジスタのサイズ表記が食い違ったコードが生成されてしまう。
            let dst_size = if opcode == "address" {
                &Size::DQ
            } else {
                &Size::DQ
            };
            self.asm_fmt
                .get_opcode_tmpl(opcode)
                .replace("{dst}", &self.get_reg(dst, dst_size))
                .replace("{src1}", &self.extract_operand_text(src1, this_is_self))
        };

        // `address`(=`lea`)は必ずここで`fmt_mnemonic_resize`を通す。
        // `lea`はアドレス(ポインタ)を求める命令なので、構造体か
        // どうかに関わらず常にポインタサイズ(8byte)として扱う。
        // ("address"はテンプレートを引くためのキーであって、
        //  実際の命令語(テンプレート中の文字列)は"lea"なので、
        //  `fmt_mnemonic_resize`にもそちらを渡す必要がある)
        if opcode == "address" {
            formated = self
                .asm_fmt
                .fmt_mnemonic_resize("lea", &formated, &Size::DQ);
        } else if self.check_node_is_struct(&src1) {
            formated = self
                .asm_fmt
                .fmt_mnemonic_resize("mov", &formated, &Size::DQ);
        } else if let Some(size) = self.check_node_is_memory_value(&src1) {
            formated = self.asm_fmt.fmt_mnemonic_resize("mov", &formated, &size);
        }

        if let Some(src2_id) = src2 {
            formated.replace("{src2}", &self.extract_operand_text(src2_id, this_is_self))
        } else {
            formated
        }
    }

    /// `idx`が(直接、あるいは`GetAddress`/`Pointer`でラップされた先に)
    /// `Inst::Struct`を指している場合、その`Inst::Struct`自身のidxを返す。
    /// `format_line`が構造体の生成を特別扱いする際、ラップされた
    /// ノードの内側までたどれるようにするためのヘルパー
    pub(super) fn resolve_struct_idx(&self, idx: &usize) -> Option<usize> {
        match &self.curr_inst[*idx] {
            inst::Inst::Struct { .. } => Some(*idx),
            inst::Inst::GetAddress(inner) | inst::Inst::Pointer(inner) => {
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
    fn check_node_is_memory_value(&self, node_idx: &usize) -> Option<Size> {
        match &self.curr_inst[*node_idx] {
            inst::Inst::MemoryValue(inst::MemoryInst::Memory { size, .. }) => Some(size.clone()),
            // `[a]`のようなポインタ関連のノードは、実際のメモリ参照
            // (`MemoryValue`)を直接ではなくラップして持っている場合がある。
            // ここで素通りしてしまうと、実際にはメモリを参照している
            // オペランドであるにも関わらずサイズが判定できず、
            // ニーモニックにサイズの接尾辞(`movl`など)が付かないまま
            // 出力されてしまう。そのため、ラップされている先を辿って
            // 判定する。
            inst::Inst::Pointer(inner) | inst::Inst::GetAddress(inner) => {
                self.check_node_is_memory_value(inner)
            }
            // `[arr 0]`のような配列の要素への参照(`Inst::InsertArr`)も
            // 同様にメモリを直接参照するオペランドになる。
            // `Inst::InsertArr`自体はサイズの情報を持っていないため、
            // 配列の変数名(`name`)から`var_hash_map`に登録済みの
            // サイズを引いて判定する。
            inst::Inst::InsertArr { name, .. } => {
                self.var_hash_map.get(name).map(|var| var.size.clone())
            }
            _ => None,
        }
    }

    /// ## 引数
    /// - reg_idx これは必ずusizeで無ければいけない、
    fn get_reg(&self, reg_idx: Option<&usize>, size: &Size) -> String {
        let num = if reg_idx.is_none() {
            self.reg_idx
        } else {
            *reg_idx.unwrap()
        };
        self.asm_fmt.get_fmt_reg(&num, &size)
    }

    pub(super) fn extract_operand_text(&mut self, parent_id: &usize, in_self_ptr: bool) -> String {
        match self.curr_inst[*parent_id].clone() {
            inst::Inst::Num { value, .. } => self.asm_fmt.get_fmt_num(&value),
            inst::Inst::GetPtr { size: _, stk } => {
                // スタック上に置かれた値そのもの(値が置かれているメモリ)
                // を指すオペランドを、`%rbp`からのオフセット`stk`を使って生成する
                self.asm_fmt.fmt_ref_operand(&"%rbp".to_string(), &stk)
            }
            inst::Inst::Param(param) => {
                // `asm_emitter/operand_txt/`に記述
                self.param_ref(&param.name)
            }
            inst::Inst::AssignVar { ref name, .. } => {
                let var_info = self.var_hash_map.get(&name.to_string()).unwrap();
                // ポインタ型の変数はアドレス(常に8byte)を保持するため、
                // 32bitレジスタ(`%ecx`など)ではなく64bitレジスタ
                // (`%rcx`など)として参照する必要がある。
                // ポインタでなければ、変数自身の型のサイズをそのまま使う
                // (以前はここが常にDQ(64bit)決め打ちになっており、
                //  例えば`int`型の変数でも`%rcx`のような64bitレジスタ
                //  として参照されてしまっていた)
                let size = if var_info.size.is_pointer().is_some() {
                    Size::DQ
                } else {
                    var_info.size.clone()
                };
                self.asm_fmt.get_fmt_reg(&var_info.reg, &size)
            }
            // 配列にアクセス
            inst::Inst::InsertArr { name, dst, index } => {
                // `asm_emitter/operand_txt/`に記述
                self.insert_arr_txt(&name, &dst, &index, in_self_ptr)
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
            inst::Inst::Struct { mem, .. } => self.emit_struct_ini_asm(mem, in_self_ptr),
            inst::Inst::MemoryValue(inst::MemoryInst::Memory { kind, size, .. }) => {
                // `asm_emitter/operand_txt/`に記述
                self.ref_mem_value_txt(&kind, &size, &parent_id)
            }
            inst::Inst::RefStruct { src, pos, .. } => {
                // `asm_emitter/operand_txt/`に記述
                self.ref_struct_txt(&src, &pos)
            }
            inst::Inst::GetAddress(index) => self.extract_operand_text(&index.clone(), in_self_ptr),
            // 配列リテラル自体を値として参照する場合
            // (例: 変数に束縛されずそのまま関数の引数などに使われる`{1,2,3}`)
            inst::Inst::InitArr(ids) => {
                // `asm_emitter/operand_txt/`に記述
                self.init_arr_txt::<false>(&ids, in_self_ptr);
                String::new()
            }
            inst::Inst::CallFunc(call_func_info) => {
                // `emit_call_func`は引数を積む`mov`と`call`命令を含む
                // 「複数行のアセンブリ文字列」を返す。これをそのまま
                // `{src1}`などのオペランドとして埋め込んでしまうと、
                // `mov call new\n, %ecx`のような壊れたコードになる。
                // そのため呼び出し自体は先に`asm_text`へ積んでおき、
                // 呼び出し規約上戻り値が置かれるレジスタ(`Ret`と同じ
                // レジスタ0番、`%eax`など)をオペランドとして返す
                if call_func_info.parent == crate::ir::IS_ASSIGN_EXPR {
                    let call_asm = self.emit_call_func(&call_func_info);
                    self.asm_text.push_str(&call_asm);
                    self.asm_fmt.get_fmt_reg(&0, &Size::DQ)
                } else {
                    String::new()
                }
            }
            // ポインタの指す先を参照する(`*p` / `[p]`)
            //
            // `GetAddress`と対になる命令で、`GetAddress`がアドレスを求める
            // 対象の値をそのまま参照する(`Pointer(GetAddress(x))`は`x`
            // 自身を指すことになるため)のと同様に、参照先の値を
            // そのまま取り出す。これにより
            // - 読み込み: `c: int = [b]` (`b`が指すメモリの値をコピーする)
            // - 書き込み: `[b] = 20` (`b`が指すメモリに値を書き込む)
            // のどちらの場合でも、実際に値が置かれているメモリ/レジスタの
            // オペランドを解決できる
            inst::Inst::Pointer(index) => self.extract_operand_text(&index.clone(), in_self_ptr),
            t => {
                if let Some(result) = self.last_inst_idx.iter().find(|i| &i.0 == parent_id) {
                    // レジスタの文字列を取得
                    self.asm_fmt.get_fmt_reg(&result.1, &Size::DQ)
                } else {
                    panic!("{:?}", t);
                }
            }
        }
    }

    pub(super) fn format_expr_inst(&mut self, expr: &inst::ExprInst) -> String {
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

        // `DEFERRED_REG_FMT_OPS`に含まれる演算子(`*`/`/`/`%`)は、
        // 結果を格納するレジスタの適切なサイズがまだ決まっていないため、
        // ひとまず番号だけのプレースホルダーを埋め込んでおく。
        // それ以外の演算子は、これまで通りその場でDD(32bit)として
        // レジスタ名を確定させる。
        let dst_text = if DEFERRED_REG_FMT_OPS.contains(&expr.kind) {
            Self::insert_fmt_reg_placeholder(&self.reg_idx)
        } else {
            self.get_reg(Some(&self.reg_idx), &Size::DQ)
        };

        let mut formated = self
            .asm_fmt
            .get_opcode_tmpl(key)
            .replace("{dst}", &dst_text)
            .replace("{src1}", &self.extract_operand_text(&expr.ls, false))
            .replace("{src2}", &self.extract_operand_text(&expr.rs, false))
            .to_string();

        // どちらかのオペランドがメモリ上の値(スタック/静的領域の変数)を
        // 参照している場合、そのサイズを採用する。そうでなければ、
        // これまで通り既定のDD(32bit)を使う。
        let resolved_size = self
            .check_node_is_memory_value(&expr.ls)
            .or_else(|| self.check_node_is_memory_value(&expr.rs));

        // どちらかのオペランドがメモリ上の値(スタック/静的領域の変数)を
        // 参照している場合、そのサイズに合わせてニーモニックへ
        // サイズの接尾辞(`movl`/`subl`など)を付ける。
        // (テンプレートは通常「一旦movでdstに値を置いてから演算する」
        //  という2行構成になっているため、両方のニーモニックを
        //  調整する必要がある)
        if let Some(ref size) = resolved_size {
            formated = self.asm_fmt.fmt_mnemonic_resize("mov", &formated, size);
            formated = self.asm_fmt.fmt_mnemonic_resize(mnemonic, &formated, size);
        }

        // `DEFERRED_REG_FMT_OPS`に含まれる演算子の場合、先ほど番号
        // だけ埋め込んでおいたプレースホルダーを、ここで確定した
        // サイズ(メモリ上の値でなければDD)を使って実際のレジスタ名へ
        // 展開する。
        if DEFERRED_REG_FMT_OPS.contains(&expr.kind) {
            let size = resolved_size.unwrap_or(Size::DQ);
            formated = self.replace_insert_fmt_reg(&formated, &size);
        }

        if self.reserved_label_name.is_some() && formated.contains("{label}") {
            // 実際に`{label}`を使うテンプレートの場合のみ予約を消費する。
            // (以前は`Option::take`を無条件に呼んでいたため、`{label}`を
            //  含まない中間の式(例: `n % 15`の剰余計算)を先に処理した
            //  際に、まだ使われていないラベルの予約が捨てられてしまい、
            //  本来ラベルを埋め込むべき比較式の`{label}`が置換されない
            //  まま出力される、という不具合があった)
            let name = self.reserved_label_name.take().unwrap();
            formated.replace("{label}", &name)
        } else {
            formated
        }
    }
}
