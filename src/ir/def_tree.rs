use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum VarType {
    Local(usize), // 変数の値や式のidx
    Param(usize), //これは、左から何番目の引数かを保存
}

#[derive(Clone, Debug, PartialEq)]
pub struct VarMetaData {
    pub attribute: VarType,
    pub size: node::TyNode,
}

impl VarMetaData {
    pub fn new(attribute: &VarType, size: &node::TyNode) -> Self {
        Self {
            attribute: attribute.clone(),
            size: size.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VarTree {
    pub hash: HashMap<String, VarMetaData>,
}

impl VarTree {
    pub fn new() -> Self {
        Self {
            hash: HashMap::new(),
        }
    }

    /// ## K
    /// - `usize`の場合 local変数
    /// - 'l' = local
    /// - 'p' = param
    pub fn push<const K: char>(
        &mut self,
        var_name: &String,
        var_index: &usize,
        var_ty: &node::TyNode,
    ) {
        let var = match K {
            'l' => VarType::Local(*var_index),
            'p' => VarType::Param(*var_index),
            _ => panic!("system err VarTree::AddのKには、`l`か`p`以外入れられません"),
        };
        self.hash
            .insert(var_name.clone(), VarMetaData::new(&var, &var_ty));
    }

    pub fn get_ty_name(&self, name: &String) -> String {
        match &self.hash.get(name).unwrap().size {
            node::TyNode::Ty(name) => name.to_string(),
            node::TyNode::Pointer { ty_name, .. } => match &**ty_name {
                node::TyNode::Ty(name) => name.to_string(),
                // `Self*`/`Self*mut`のように、`Self`を指す
                // ポインタ型の場合も、実際の構造体名を返す
                node::TyNode::SelfTy(name) => name.to_string(),
                _ => panic!(),
            },
            node::TyNode::SelfTy(name) => name.to_string(),
            t => panic!("{:?}", t),
        }
    }

    #[inline(always)]
    pub fn get_ty_node(
        &self, 
        var_name: &String
    ) -> Option<node::TyNode> {
        self.hash
            .get(var_name)
            .map(|var| var.size.clone())
    }

    /// 指定された変数が`Self`型、または`Self`を指すポインタ型
    /// (`Self*` / `Self*mut`)かどうかを判定する
    pub fn is_self_ty(&self, name: &String) -> bool {
        self.hash.get(name).is_some_and(|v| match &v.size {
            node::TyNode::SelfTy(_) => true,
            node::TyNode::Pointer { ty_name, .. } => {
                matches!(**ty_name, node::TyNode::SelfTy(_))
            }
            _ => false,
        })
    }

    /// 指定された変数が引数か、ローカル変数かなどを返す
    pub fn get(&self, name: &String) -> &VarType {
        &self.hash.get(name).expect(name).attribute
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructTree {
    tree: HashMap<String, node::StructDefine>,
}

impl StructTree {
    pub fn new() -> Self {
        Self {
            tree: HashMap::new(),
        }
    }

    pub fn add(&mut self, info: &node::StructDefine) {
        self.tree.insert(info.name.to_string(), info.clone());
    }

    pub fn get(&self, name: &String) -> Option<&node::StructDefine> {
        self.tree.get(name)
    }

    /// 任意の構造体を指定し、その構造体にあるメンバー
    /// のバイトの位置を取得し返す関数
    pub fn get_pos(
        &self,
        // 構造体の名前
        name: &String,
        field_name: &String,
    ) -> usize {
        let mut byte_counter = 0;
        for member in self.tree.get(name).expect(name).fields.iter() {
            byte_counter += types::Size::new(&member.ty)
                .unwrap()
                .to_bytes();
            if member.name == field_name.as_str() {
                return byte_counter;
            }
        }

        panic!();
    }

    pub fn get_mem_size(
        &self,
        name: &String,
        field_name: &String,
    ) -> types::Size {
        let ty = self.tree
            .get(name)
            .unwrap()
            .fields
            .iter()
            .find(|v| &v.name == field_name)
            .unwrap()
            .ty
            .clone();
        types::Size::new(&ty).unwrap()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FuncDefInfo {
    pub module: Option<String>,
    pub args: Vec<node::ArgsNode>,
    pub body: Vec<inst::Inst>,
    pub ret_ty: Option<node::TyNode>,
    pub public: bool,
    pub stk_size: usize,
}

impl FuncDefInfo {
    /// 第一引数が`Self`型、または`Self`を指すポインタ型
    /// (`self: Self` / `self: Self*` / `self: Self*mut`)かどうかを判定する
    pub fn first_param_is_self(&self) -> bool {
        self.args.get(0).is_some_and(|f| match &f.ty {
            node::TyNode::SelfTy(..) => true,
            node::TyNode::Pointer { ty_name, .. } => {
                matches!(**ty_name, node::TyNode::SelfTy(..))
            }
            _ => false,
        })
    }

    pub fn get_ret_ty(&self) -> crate::gen::SelfPtrInfo {
        if self.ret_ty.is_none() {
            return types::Size::Void.wrap_dst_size();
        }
        match self.ret_ty.as_ref().unwrap() {
            node::TyNode::SelfTy(..) => None,
            ty => Some(types::Size::new(ty).unwrap()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FuncTree {
    pub func: HashMap<String, FuncDefInfo>,
}

impl FuncTree {
    pub fn new() -> Self {
        Self {
            func: HashMap::new(),
        }
    }

    /// `func`ハッシュマップに登録する際のキーを生成する
    ///
    /// 構造体のメゾットを展開した関数など、モジュール名を持つ
    /// 関数は`モジュール名::関数名`をキーにする(`StructName::method`の
    /// ように同名のメゾットが別の構造体にあっても衝突しないようにする)。
    /// モジュール名を持たない、通常の(トップレベルの)関数は
    /// そのまま関数名だけをキーにする
    fn make_key(name: &String, module_name: Option<&String>) -> String {
        match module_name {
            Some(module) => format!("{}::{}", module, name),
            None => name.clone(),
        }
    }

    pub fn get(&mut self, name: &String, module_name: Option<&String>) -> Option<FuncDefInfo> {
        self.func.get(&Self::make_key(name, module_name)).cloned()
    }

    pub fn add(
        &mut self,
        body: Vec<inst::Inst>,
        meta_data: &node::FuncDefine,
        ret_ty: &node::TyNode,
        stk_size: usize,
    ) {
        let key = Self::make_key(&meta_data.name, meta_data.module.as_ref());
        self.func.insert(
            key,
            FuncDefInfo {
                // 構造体のメゾットとして展開された関数の場合、
                // 属している構造体の名前がここに入る
                module: meta_data.module.clone(),
                args: meta_data.params.clone(),
                body,
                ret_ty: Some(ret_ty.clone()),
                public: meta_data.public,
                stk_size,
            },
        );
    }
}

#[derive(Clone, Debug)]
pub struct FuncDefMetaData {
    module: Option<String>,
    pub name: String,
    params: Vec<node::ArgsNode>,
    ret_ty: Option<node::TyNode>,
    // `#include`で他のファイルから読み込む際に、公開(pub)された
    // 関数かどうかを判定するために使う
    pub public: bool,
}

impl FuncDefMetaData {
    /// moduleは自分自身がどのモジュールに属しているか
    /// Noneの場合は、#includeで関数の名前ごと指定しているか
    /// 自分のファイルの中にあるかのどちらか
    pub fn new(info: &node::FuncDefine, module: Option<&String>) -> Self {
        Self {
            module: module.map(|v| v.clone()),
            name: info.name.clone(),
            params: info.params.clone(),
            ret_ty: Some(info.ret_ty.clone()),
            public: info.public,
        }
    }

    pub fn add_self_module_name(&mut self, self_name: &String) {
        self.module = Some(self_name.to_string());
    }

    /// この関数がどのモジュール名で登録されているかを返す
    /// (`#include`でモジュール名を指定せず取り込んだ関数は`None`)
    pub fn module(&self) -> Option<&String> {
        self.module.as_ref()
    }

    pub fn gen(&self, stk_size: usize) -> FuncDefInfo {
        FuncDefInfo {
            module: None,
            args: self.params.clone(),
            body: Vec::new(),
            ret_ty: self.ret_ty.clone(),
            public: true,
            stk_size,
        }
    }
}
