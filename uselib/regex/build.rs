use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").unwrap()
    );

    let build_py = manifest_dir.join("build.py");

    // ターゲットアーキテクチャに応じて asm/x64 か asm/arm64 を選択する
    // (以前は "x64" 固定で、arm64 向けにビルドしても x64 のアセンブリが
    //  使われてしまっていた)
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH")
        .unwrap_or_else(|_| "x86_64".to_string());
    let arch_arg = if target_arch == "aarch64" { "arm64" } else { "x64" };

    Command::new("python3")
        .arg(&build_py)
        .arg(arch_arg)
        .status()
        .expect("failed to execute build.py");

    println!(
        "cargo:rustc-link-search=native={}",
        manifest_dir.display()
    );

    println!("cargo:rustc-link-lib=static=regex");
    println!("cargo:rustc-link-search=native=uselib/regex");

    println!("cargo:rerun-if-changed=src/c");
    println!("cargo:rerun-if-changed=src/asm");
}