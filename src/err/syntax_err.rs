//!    - [`SyntaxErrKind`] : トークン管理・式解析全般の構文エラー

//! ## エラーの組み立て方
//! 各`kind`を直接組み立てて`ErrLoc`を手で埋めるのは面倒なので、
//! 代わりに次のマクロを使う。マクロの中で[`func_name!`]を展開する
//! ことで、「そのマクロを実際に書いた場所を囲む関数の名前」を
//! 自動的に取得できる。
//!
//! - [`syntax_err!`]      : `SyntaxErrKind`から`ErrKind`を組み立てる

use super::*;

use crate::lex;
use std::fmt;
// ---------------------------------------------------------------------
// 構文全般(トークン管理・式)のエラー
// ---------------------------------------------------------------------

/// トークン管理・式解析など、構文解析全般で発生するエラーの種類。
///
/// 位置情報や関数名は持たず、「何が起きたか」だけを表す。
/// 位置・関数名は[`ErrLoc`]としてまとめて[`SyntaxErr`]に持たせる。
#[derive(Debug, Clone, PartialEq)]
pub enum SyntaxErrKind {
    /// 期待していたトークンがある前に、トークン列が終了した(EOF)
    TknIsEof { expected: Vec<&'static str> },
    /// 現在のトークンが、期待していたトークンと異なる
    UnexpectedTkn {
        /// 実際に出現したトークン
        found: lex::Tkn,
        /// 期待していたトークン
        expected: lex::Tkn,
        /// このトークンの直前の構文(エラーメッセージ用の文脈)
        context: lex::Tkn,
    },
    /// キーワードの直後に、期待していたトークンが来なかった
    UnexpectTknAfterKeyword {
        /// 直前のキーワード
        keyword: lex::Tkn,
        /// 期待していたトークン(の説明)
        expected: Vec<&'static str>,
        /// 実際に出現したトークン
        found: lex::Tkn,
    },
    /// スコープ(`{ .. }`)が`}`で閉じられていない
    UnenclosedScope {
        /// 閉じるべきだった構文(例: `cond`)。分からない場合は`None`
        target: Option<lex::Tkn>,
    },
    /// 見つかるべきトークンが見つからなかった
    NotFoundTkn(lex::Tkn),
    /// 式の途中で、予期しないトークンが出現した
    UnexpectTknInExpr { found: lex::Tkn },
    /// 式の解析中にトークン列が終了した(EOF)
    TknIsEofInExpr,
    /// `cond`(match)式特有の構文エラー
    Cond(CondErrKinds),
    /// 文(stmt)/トップレベルの解析中に、予期しないトークンが出現した
    UnexpectTknInStmt { found: lex::Tkn },
    /// ある種類のトークン(名前・数字・記号など)が期待されていた
    /// 場所に、それとは異なるトークンが出現した(汎用)
    ///
    /// [`UnexpectedTkn`](Self::UnexpectedTkn)と違い、期待する
    /// トークンが1種類の`lex::Tkn`に定まらない場合(例:
    /// 「名前」「数字」「`,`か`)`」など)に使う
    ExpectedKind {
        /// 期待していたものの説明(例: `"name"`, `"number"`, `"( or <"`)
        expected: &'static str,
        /// 実際に出現したトークン
        found: lex::Tkn,
    },
    /// 構文としては認識できるが、まだパーサーが対応していない機能
    NotImplemented {
        /// 未対応の機能名(例: `"ジェネリクス"`)
        feature: &'static str,
    },
}

impl fmt::Display for SyntaxErrKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TknIsEof { expected } => write!(
                f,
                "トークン列が予期せず終了しました(期待していたトークン: {})",
                expected.join(" / ")
            ),
            Self::UnexpectedTkn {
                found,
                expected,
                context,
            } => write!(
                f,
                "`{:?}`の後には`{:?}`が必要ですが、`{:?}`が見つかりました",
                context, expected, found
            ),
            Self::UnexpectTknAfterKeyword {
                keyword,
                expected,
                found,
            } => write!(
                f,
                "`{:?}`の後には{}のいずれかが必要ですが、`{:?}`が見つかりました",
                keyword,
                expected.join(" / "),
                found
            ),
            Self::UnenclosedScope { target } => match target {
                Some(t) => write!(f, "`{:?}`のスコープが`}}`で閉じられていません", t),
                None => write!(f, "スコープが`}}`で閉じられていません"),
            },
            Self::NotFoundTkn(t) => write!(f, "`{:?}`が見つかりません", t),
            Self::UnexpectTknInExpr { found } => {
                write!(f, "式の中で予期しないトークン`{:?}`が見つかりました", found)
            }
            Self::TknIsEofInExpr => write!(f, "式の解析中にトークン列が終了しました"),
            Self::Cond(kind) => write!(f, "{}", kind),
            Self::UnexpectTknInStmt { found } => {
                write!(f, "文の中で予期しないトークン`{:?}`が見つかりました", found)
            }
            Self::ExpectedKind { expected, found } => write!(
                f,
                "{}が必要ですが、`{:?}`が見つかりました",
                expected, found
            ),
            Self::NotImplemented { feature } => {
                write!(f, "`{}`はまだ実装されていません", feature)
            }
        }
    }
}

/// 位置情報付きの構文エラー。
///
/// `expr.rs`/`expr/cond.rs`など式まわりの解析関数は、
/// この型そのものを`Result`の`Err`として直接返す。
#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxErr {
    pub kind: SyntaxErrKind,
    pub loc: ErrLoc,
}

/// `SyntaxErr`の別名。式まわりの解析関数(`expr.rs`など)では
/// 慣習的にこちらの名前で参照する
pub type SynErrs = SyntaxErr;

impl fmt::Display for SyntaxErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "構文エラー: {} [{}]", self.kind, self.loc)
    }
}

/// [`SyntaxErrKind`]から[`ErrKind`]を組み立てるマクロ。
///
/// ```ignore
/// crate::syntax_err!(self.build_err_span(), err::SyntaxErrKind::TknIsEof { expected })
/// ```
/// のように、位置(`Span`)と`SyntaxErrKind`の値を渡す。
/// 戻り値は`Result<T, err::ErrKind>`(常に`Err`)で、`T`は呼び出し側の
/// 文脈から推論される。
#[macro_export]
macro_rules! syntax_err {
    ($span:expr, $kind:expr) => {
        Err::<_, $crate::err::ErrKind>($crate::err::ErrKind::Syntax($crate::err::SyntaxErr {
            kind: $kind,
            loc: $crate::err::ErrLoc::new($span, $crate::func_name!()),
        }))
    };
}

// ---------------------------------------------------------------------
// `cond`(match)式特有のエラー
// ---------------------------------------------------------------------

/// `cond`(match)式の解析中に発生する、構文エラーの種類
#[derive(Debug, Clone, PartialEq)]
pub enum CondErrKinds {
    /// `cond`式のスコープが`{`で始まっていない
    CondExprScopeStartLBrace,
    /// パターンの後に`{`が来なかった
    CondExprPatternLBrace,
    /// `|`(else)の後に`{`が来なかった
    CondElseLBrace,
    /// `|`(else)が見つからなかった
    CondElseNotFound,
}

impl fmt::Display for CondErrKinds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::CondExprScopeStartLBrace => "`cond`式のスコープは`{`で始まる必要があります",
            Self::CondExprPatternLBrace => "パターンの後には`{`が必要です",
            Self::CondElseLBrace => "`|`(else)の後には`{`が必要です",
            Self::CondElseNotFound => "`cond`式に`|`(else)が見つかりません",
        };
        write!(f, "{}", msg)
    }
}

/// [`CondErrKinds`]から[`ErrKind`]を組み立てるマクロ。
///
/// 内部的には`SyntaxErrKind::Cond`として[`syntax_err!`]に委譲する。
/// ```ignore
/// crate::cond_err!(self.build_err_span(), CondExprScopeStartLBrace)?
/// ```
#[macro_export]
macro_rules! cond_err {
    ($span:expr, $kind:ident) => {
        $crate::syntax_err!(
            $span,
            $crate::err::SyntaxErrKind::Cond($crate::err::CondErrKinds::$kind)
        )
    };
}

// ---------------------------------------------------------------------
// プリプロセッサ特有のエラー
// ---------------------------------------------------------------------

/// プリプロセッサ(`#include`/`#asm`など)の解析中に発生するエラーの種類
#[derive(Debug, Clone, PartialEq)]
pub enum PreprocErrs {
    /// `${}`の中身が空だった
    EmptyAsmOperand,
    /// `${...}`の解析後、余分なトークンが残っている
    UnexpectedTrailingTokenInAsmOperand,
    /// アドレス取得(`[..]`)を閉じる`]`が見つからない
    ExpectedRBracketInAsmOperand,
    /// 括弧(`(..)`)を閉じる`)`が見つからない
    ExpectedRParenInAsmOperand,
    /// オペランドとして解釈できないトークンが出現した
    UnexpectedTokenInAsmOperand,
    /// `.`の後にメンバー名(名前トークン)が来なかった
    ExpectedMemberNameInAsmOperand,
    /// `#include`のモジュールパスの要素が見つからない
    ExpectedPathSegment,
    /// `#asm`の後に`(`が来なかった
    ExpectedLParenAfterAsm,
    /// `#asm(...)`にアセンブラの名前が指定されていない
    NotFoundAsmName,
    /// `#asm(name`の後に`)`が来なかった
    ExpectedRParenAfterAsm,
}

impl fmt::Display for PreprocErrs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::EmptyAsmOperand => "`${}`の中身が空です",
            Self::UnexpectedTrailingTokenInAsmOperand => {
                "`${...}`の中に、式として解析できない余分なトークンがあります"
            }
            Self::ExpectedRBracketInAsmOperand => "`]`が必要です",
            Self::ExpectedRParenInAsmOperand => "`)`が必要です",
            Self::UnexpectedTokenInAsmOperand => {
                "インラインアセンブラのオペランドとして解釈できないトークンです"
            }
            Self::ExpectedMemberNameInAsmOperand => "`.`の後にはメンバー名が必要です",
            Self::ExpectedPathSegment => "モジュールパスの要素が見つかりません",
            Self::ExpectedLParenAfterAsm => "`#asm`の後には`(`が必要です",
            Self::NotFoundAsmName => "`#asm(...)`にはアセンブラの名前が必要です",
            Self::ExpectedRParenAfterAsm => "`#asm(name`の後には`)`が必要です",
        };
        write!(f, "{}", msg)
    }
}

/// 位置情報付きのプリプロセッサエラー
#[derive(Debug, Clone, PartialEq)]
pub struct PreprocErrDetail {
    pub kind: PreprocErrs,
    pub loc: ErrLoc,
}

impl fmt::Display for PreprocErrDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "プリプロセッサのエラー: {} [{}]", self.kind, self.loc)
    }
}

/// `Parser`(正確には`build_err_span`メソッドを持つ値)から、
/// [`PreprocErrs`]の[`ErrKind`]を作りその場で`return`するマクロ。
///
/// ```ignore
/// crate::preproc_err!(self, ExpectedRParenInAsmOperand);
/// ```
#[macro_export]
macro_rules! preproc_err {
    ($parser:expr, $kind:ident) => {
        return Err($crate::err::ErrKind::Preproc($crate::err::PreprocErrDetail {
            kind: $crate::err::PreprocErrs::$kind,
            loc: $crate::err::ErrLoc::new($parser.build_err_span(), $crate::func_name!()),
        }))
    };
}

/// 位置(`Span`)を直接指定して、[`PreprocErrs`]の[`ErrKind`]を
/// (`return`せずに)式として組み立てるマクロ。
///
/// `Parser`のインスタンスがまだ存在しない場合など、
/// `build_err_span`が使えない場面で使う。
///
/// ```ignore
/// return Err(crate::preproc_err_at!(err::Span::new(&0, &0), EmptyAsmOperand));
/// ```
#[macro_export]
macro_rules! preproc_err_at {
    ($span:expr, $kind:ident) => {
        $crate::err::ErrKind::Preproc($crate::err::PreprocErrDetail {
            kind: $crate::err::PreprocErrs::$kind,
            loc: $crate::err::ErrLoc::new($span, $crate::func_name!()),
        })
    };
}