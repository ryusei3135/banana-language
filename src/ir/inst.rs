use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Add,
    Sub,
    Mul,
    Div,
    Surplus,
    LessThen,
    GreaterThen,
    Equal,
    NotEq,
}

pub type ValueId = usize;

#[derive(Clone, Debug, PartialEq)]
pub struct ExprInst {
    pub dst: ValueId,
    pub ls: ValueId,
    pub rs: ValueId,
    pub kind: ExprKind,
}

impl ExprInst {
    pub fn new(self) -> Inst {
        Inst::Expr(self)
    }
}

/// 引数の定義の情報
#[derive(Clone, Debug, PartialEq)]
pub struct ParamMetaData {
    /// 引数の名前
    pub name: String,
    /// 何番目の引数か
    /// アセンブリ言語でレジスタを指定するため
    pub num: usize,
    /// 値があるindex
    pub dst: usize,
    pub ty: types::Size,
}

impl ParamMetaData {
    pub fn new(
        name: String, 
        num: usize, 
        dst: usize, 
        ty: &node::TyNode
    ) -> Self {
        Self { 
            name, 
            num, 
            dst, 
            ty: types::Size::new(&ty).unwrap()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CallFuncMetaData {
    pub path: Vec<String>,
    pub public: bool,
    pub name: String,
    pub params: Vec<ValueId>,

    /// 関数を呼ぶ際に、代入する変数があるかをbooleanで表現
    /// 呼び出すときに、変数へ代入するなら、false
    pub parent: bool,
    /// アセンブリ言語を出力する際に、スタックを確保するためのサイズ
    /// Someの場合、スタックを確保する
    pub stk_capacity: Option<usize>,
}

impl CallFuncMetaData {
    pub fn new(name: String, start_expr: bool, stk_capacity: Option<usize>) -> Self {
        Self {
            path: Vec::new(),
            public: false,
            name,
            params: Vec::new(),
            parent: start_expr,
            stk_capacity,
        }
    }

    pub fn insert_param_parent_id(&mut self, value_id: ValueId) {
        self.params.push(value_id);
    }

    pub fn module(&mut self, path: &Vec<String>) {
        self.path = path.clone();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MemoryKind {
    Stack,
    Static,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MemoryInst {
    Member {
        parent: String,
        value_idx: usize,
        size: types::Size,
    },
    Memory {
        name: String,
        size: types::Size,
        src: Vec<usize>,
        kind: MemoryKind,
        dst: usize,
    },
    Byte(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inst {
    /// ポインタの対象のidx
    Pointer(usize),
    GetAddress(usize),
    GetPtr {
        size: usize,
        stk: usize,
    },

    ExternFunc(String),
    Expr(ExprInst),
    Block(String),
    Jmp(String),
    ExpectJmp(String), // ジャンプする場所
    Mov {
        name: Option<String>,
        size: types::Size,
        dst: ValueId,
        src: ValueId,
    },
    /// 構造体のメンバーにアクセスする
    RefStruct {
        /// 構造体のアドレスがある、場所
        src: String,
        /// 指定された、メンバーの場所
        pos: usize,
        size: types::Size,
    },
    Stacks {
        size: usize,
    },
    Num {
        dst: ValueId,
        value: String,
        size: types::Size,
    },
    Str {
        dst: ValueId,
        value: String,
    },
    InitArr(Vec<usize>),
    /// 配列にアクセスするノード
    InsertArr {
        name: String,
        dst: usize,
        index: usize,
    },
    CallFunc(CallFuncMetaData),
    Comple {
        name: String,
        /// inlineアセンブラの各行: `(プレースホルダー入りの文字列, オペランドのidx)`
        lines: Vec<(String, Vec<usize>)>,
    },
    AssignVar {
        name: String,
        /// 代入先のノードがあるindex
        dst: usize,
        value: usize,
    },
    Param(ParamMetaData),
    Ret(ValueId),
    Struct {
        name: String,
        mem: Vec<MemoryInst>,
        is_self: bool,
    },
    MemoryValue(MemoryInst),
}

impl Inst {
    pub fn is_pointer(&self) -> bool {
        match self {
            Self::Pointer(..) => true,
            Self::GetAddress(..) => true,
            _ => false,
        }
    }

    pub fn get_param_ty(&self) -> Option<types::Size> {
        match &self {
            inst::Inst::Param(p) => Some(p.clone().ty),
            inst::Inst::GetAddress(..) => Some(types::Size::DQ),
            inst::Inst::Num { size, .. } => Some(size.clone()),
            inst::Inst::RefStruct { size, .. } => Some(size.clone()),
            inst::Inst::MemoryValue(inst::MemoryInst::Memory{ size, .. }) => Some(size.clone()),
            inst::Inst::Mov { size, .. } => Some(size.clone()),
            inst::Inst::Str {..} => Some(types::Size::Pointer {is_const:false, ty: Box::new(types::Size::DB)}),
            t => {
                panic!("{:?}", t);
            }
        }
    }

    pub fn gen_num(value: &str, size: &types::Size, dst: usize) -> Self {
        match size {
            types::Size::DB => {
                value.parse::<u8>().unwrap();
            }
            types::Size::DW => {
                value.parse::<u16>().unwrap();
            }
            types::Size::DD => {
                value.parse::<u32>().unwrap();
            }
            types::Size::DQ => {
                value.parse::<u64>().unwrap();
            }
            types::Size::Struct(_) => {
                panic!("gen_num: 構造体型に数値を直接代入することはできません");
            }
            types::Size::Array { size, .. } => {
                Self::gen_num(&value, &size, dst);
            }
            types::Size::Pointer { .. } => {
                value.parse::<u64>().unwrap();
            }
            types::Size::Void => panic!(),
        }
        Self::Num {
            dst,
            value: value.to_string(),
            size: size.clone(),
        }
    }
}
