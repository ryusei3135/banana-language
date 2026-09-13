mod asm_setting;
mod err;
mod gen;
mod ir;
mod lex;
mod macros;
mod parse;

use std::{env, fs, process};
pub use parse::node;

// ファイルやオプション管理
pub mod cmd_line_args {
    use super::*;

    pub enum OptFlags {
        FmtAsm,
        SetFile,
    }

    pub struct OptSettings {
        pub fmt_name: Option<String>,
        pub file_name: Option<String>,

        opt_flags: Option<OptFlags>,
    }

    impl OptSettings {
        pub fn new(first_flag: OptFlags) -> Self {
            Self {
                fmt_name: None,
                file_name: None,

                opt_flags: Some(first_flag),
            }
        }

        pub fn new_file(&self, file_name: &String) -> Self {
            Self {
                fmt_name: self.fmt_name.clone(),
                file_name: Some(file_name.clone()),
                opt_flags: Some(OptFlags::SetFile),
            }
        }

        /// 値を渡す、フラグがすでに立っているなら成功
        pub fn set_value(
            &mut self, 
            value: String
        ) -> Result<(), err::opt::OptErrs> {
            if let Some(flag) = self.opt_flags.take() {
                let _ = match flag {
                    OptFlags::FmtAsm => self.fmt_name.insert(value),
                    OptFlags::SetFile => self.file_name.insert(value),
                };
                Ok(())
            } else {
                Err(err::opt::OptErrs::OptFlags)
            }
        }

        /// フラグを立てる
        /// もしフラグがすでに立っている場合はエラーになる
        pub fn set_flag(
            &mut self, 
            flag_name: OptFlags
        ) -> Result<(), err::opt::OptErrs> {
            if self.opt_flags.is_none() {
                let _ = self.opt_flags.insert(flag_name);
                Ok(())
            } else {
                Err(err::opt::OptErrs::OptFlags)
            }
        }
    }

    /// オプション管理
    pub fn mng_opt_cmd(args: &Vec<String>) -> OptSettings {
        let mut settings = OptSettings::new(OptFlags::SetFile);

        for (index, opt) in args.iter().enumerate() {
            // 1以下の数はオプションをつけれない
            match &index {
                0 => continue,
                1 => {
                    if let Err(e) = settings.set_value(opt.clone()) {
                        eprintln!("警告: コマンドライン引数`{}`を無視しました: {}", opt, e);
                    }
                    continue;
                }
                _ => {}
            }

            match opt.as_str() {
                "-f" => {
                    if let Err(e) = settings.set_flag(OptFlags::FmtAsm) {
                        eprintln!("警告: オプション`-f`を無視しました: {}", e);
                    }
                }
                // フラグ以外の文字
                _ => {
                    if let Err(e) = settings.set_value(opt.clone()) {
                        eprintln!("警告: コマンドライン引数`{}`を無視しました: {}", opt, e);
                    }
                }
            }
        }
        settings
    }
}

/// もらった情報で、データを生成
/// ## 戻り値
/// 関数の戻り値は呼び出し元に現在の公開されている
/// 関数の情報の配列を返す
///
/// ## Errors
/// - ファイル名が指定されていない場合
/// - ソースファイルの読み込みに失敗した場合
/// - 字句解析・構文解析・IR生成のいずれかに失敗した場合
/// - アセンブリ言語ファイルの書き込みに失敗した場合
pub fn build(
    settings: &cmd_line_args::OptSettings,
) -> Result<Vec<ir::def_tree::FuncDefMetaData>, Box<dyn std::error::Error>> {
    // 初期化
    let file_name = settings
        .file_name
        .as_ref()
        .ok_or_else(|| "ファイル名が指定されていません".to_string())?;

    let content = fs::read_to_string(file_name)
        .map_err(|e| format!("ファイル`{}`を読み込めません: {}", file_name, e))?;

    let mut lexer = lex::Lexer::new();
    let mut parser = parse::Parser::new();
    let mut ir_builder = ir::IR::new();

    // アセンブリ言語のデータを作成
    lexer.analy(&content)?;

    let nodes = parser.parser(lexer.gen_tkns.clone())?;

    let func_def_meta_data = ir_builder
        .builder(
            &nodes,
            #[cfg(not(test))]
            &settings,
        )
        .map_err(|e| format!("IRの生成に失敗しました: {:?}", e))?;

    let asm_text = asm_setting::gen_asm_text(
        ir_builder.func_tree,
        &ir_builder.extern_funcs,
        &ir_builder.public_func_tree,
        &settings.fmt_name,
    );

    // 出力先のアセンブリ言語のファイル
    let asm_file = file_name.replace(".hexl", "");
    fs::write(format!("{}.s", asm_file), asm_text)
        .map_err(|e| format!("ファイル`{}.s`へ書き込めません: {}", asm_file, e))?;

    Ok(func_def_meta_data)
}

fn main() -> process::ExitCode {
    let args: Vec<String> = env::args().collect();
    // オプションなどの設定
    let settings = cmd_line_args::mng_opt_cmd(&args);

    asm_setting::load_setting();

    match build(&settings) {
        Ok(_) => process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("エラー: {}", e);
            process::ExitCode::FAILURE
        }
    }
}