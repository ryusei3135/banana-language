use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum Size {
    DB,
    DW,
    DD,
    DQ,
    Struct(Vec<Box<(String, Size)>>),
    Pointer { ty: Box<Size>, is_const: bool },
    Array { size: Box<Size>, len: usize },
    Void,
}

impl Size {
    /// 組み込みの型(byte/u16/int/u64)のみを解決する
    /// 構造体や列挙型などのユーザー定義の型を解決する場合は
    /// `builder::IR::size_of` を使用する
    pub fn new(
        ty: &node::TyNode
    ) -> Result<Self, err::undef::UndefKind> {
        let size_ty = match ty {
            node::TyNode::Ty(ref ty_name) => {
                embe_ty_sort(ty_name)?
            }
            // スタック/静的領域の型は、要素の型と同じサイズを持つ
            node::TyNode::Stack { name, .. }
            | node::TyNode::Static { name, .. } => {
                Self::new(&node::TyNode::Ty(name.clone()))?
            }
            node::TyNode::Pointer {
                is_const, 
                ty_name 
            } => Self::Pointer {
                ty: Box::new(Self::new(&*ty_name)?),
                is_const: is_const.clone(),
            },
            node::TyNode::SelfTy(..) => {
                Self::DQ
            }
            _ => panic!(),
        };
        Ok(size_ty)
    }

    pub fn is_pointer(&self) -> Option<Size> {
        if let Self::Pointer { ty, .. } = self {
            Some(*ty.clone())
        } else {
            None
        }
    }

    /// ポインタ型を作成する
    pub fn build_ptr_ty(ty: &node::TyNode) -> Self {
        Self::Pointer {
            ty: Box::new(Self::new(&ty).unwrap()),
            is_const: false,
        }
    }

    /// 組み込み型かどうかを判定する
    pub fn is_builtin_ty_name(name: &str) -> bool {
        matches!(name, "byte" | "i16" | "int" | "i64")
    }

    /// このサイズがバイト単位で何バイトかを返す
    pub fn to_bytes(&self) -> usize {
        match self {
            Self::DB => 1,
            Self::DW => 2,
            Self::DD => 4,
            Self::DQ => 8,
            Self::Array { size, len } => size.to_bytes() * len,
            Self::Pointer { ty, .. } => (*ty).to_bytes(),
            Self::Struct(struct_size) => {
                let mut size_counter = 0;
                for mem in struct_size.iter() {
                    size_counter += mem.1.to_bytes();
                }
                size_counter
            }
            Self::Void => panic!(),
        }
    }

    #[inline(always)]
    pub fn wrap_dst_size(&self) -> crate::gen::SelfPtrInfo {
        Some(self.clone())
    }

    #[inline(always)]
    pub fn wrap_ok<E>(self) -> Result<Self, E> {
        Ok(self)
    }
}

#[inline(always)]
fn embe_ty_sort(
    ty_name: &String
) -> Result<Size, err::undef::UndefKind> {
    match ty_name.as_str() {
        "byte" => Size::DB,
        "i16" => Size::DW,
        "int" => Size::DD,
        "i64" => Size::DQ,
        _ => {
            return Err(err::undef::UndefKind::UndefVarTy);
        }
    }
    .wrap_ok::<err::undef::UndefKind>()
}

impl IR {
    /// 型からサイズを求める
    ///
    /// 組み込み型(`byte`/`u16`/`int`/`u64`)は`types::Size::new`と
    /// 同じ結果を返すが、構造体・列挙型などのユーザー定義の型名が渡された
    /// 場合は`struct_tree`/`enum_tree`を参照して解決する
    pub(super) fn size_of(&self, ty: &node::TyNode) -> types::Size {
        match ty {
            node::TyNode::Ty(name) => {
                if types::Size::is_builtin_ty_name(name) {
                    return types::Size::new(ty).unwrap();
                }

                if self.enum_tree.contains_key(name) {
                    // 列挙型は現在、タグ(整数値)として扱う
                    return types::Size::DD;
                }

                if let Some(struct_def) = self.struct_tree.get(&name) {
                    let fields = struct_def
                        .fields
                        .iter()
                        .map(|field| Box::new((field.name.clone(), self.size_of(&field.ty))))
                        .collect();
                    return types::Size::Struct(fields);
                }

                panic!("未定義の型です: {}", name);
            }
            node::TyNode::Pointer { is_const, ty_name } => types::Size::Pointer {
                ty: Box::new(self.size_of(ty_name)),
                is_const: is_const.clone(),
            },
            // スタック/静的領域の型は、要素の型と同じサイズを持つ
            node::TyNode::Stack { name, len } | node::TyNode::Static { name, len } => {
                let size = self.size_of(&node::TyNode::Ty(name.clone()));
                // 配列の作成
                if len >= &1 {
                    types::Size::Array {
                        size: Box::new(size),
                        len: *len,
                    }
                } else {
                    size
                }
            }
            node::TyNode::RefTy(inner) => self.size_of(inner),
            // `Self`はIRへ変換する前に、実際の構造体の型
            // (`node::TyNode::Ty`)やポインタ型へ解決されている必要がある
            node::TyNode::SelfTy(name) => self.size_of(&node::TyNode::Ty(name.to_string())),
        }
    }
}
