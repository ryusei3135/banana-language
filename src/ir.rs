use crate::node;
use std::collections::HashMap;
use std::mem;

pub mod builder;
/// 変数や関数、構造体などの、定義を一時的に
/// 保存する構造体を提供するモジュール
pub mod def_tree;
pub mod inst;
mod param;

mod ty_checker;
pub mod types;

use crate::err;

pub struct IR {
    pub var_tree: def_tree::VarTree,
    pub extern_funcs: Vec<inst::Inst>,
    id_counter: usize,
    func_ret_ty: Option<node::TyNode>,
    ir_tree: Vec<inst::Inst>,
    pattern_labels: usize,

    this_is_self: bool,
    /// 式や文を生成する際に、一番最初のノードの場合のみtrue
    /// 関数を呼ぶノードが変数に戻り値を代入市内債などに使う
    expr_counter: usize,
    // 関数の情報
    pub func_tree: def_tree::FuncTree,
    // 外部の関数の情報
    extern_func_tree: Vec<def_tree::FuncDefMetaData>,
    // 自身が公開する関数の配列:
    pub public_func_tree: Vec<String>,
    define_meta_data: Vec<def_tree::FuncDefMetaData>,
    // 定義済みの構造体の情報
    pub struct_tree: def_tree::StructTree,
    // 定義済みの列挙型の情報
    pub enum_tree: HashMap<String, node::EnumDefine>,
    stk_counter: usize,
}

pub const IS_ASSIGN_EXPR: bool = true;
pub const IS_NOT_ASSIGN_EXPR: bool = false;
