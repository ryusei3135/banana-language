//! このモジュールは、IRで生成したものを
//! 渡されたフォーマットのアセンブリ言語に変換するApiを提供する

/// self.format_lineで構造体のポインタを渡すところがある
mod asm_emitter;
mod call_func;
mod inline_asm;
mod mng_fmt;
mod emit_fn_name;

use crate::ir::{def_tree, inst, types::Size};
use std::collections::HashMap;
use std::mem;

pub use asm_emitter::AsmEmitter;

pub type SelfPtrInfo = Option<Size>;
