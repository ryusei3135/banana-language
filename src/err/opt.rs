use std::fmt;

/// コマンドライン引数のパース中に発生するエラー
#[derive(Debug, Clone, PartialEq)]
pub enum OptErrs {
    /// 指定されたオプションが存在しない
    NotFoundOpt(String),
    OptFlags,
}

impl fmt::Display for OptErrs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFoundOpt(opt) => write!(f, "不明なオプションです: `{}`", opt),
            Self::OptFlags => write!(f, "オプションの指定方法が正しくありません"),
        }
    }
}

impl std::error::Error for OptErrs {}