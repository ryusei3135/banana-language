"""
uselib トップレベルビルドスクリプト
====================================
外部ライブラリ(libc等)に依存しない自前実装の3コンポーネントを
まとめてビルドする。

  mem/   ... mem/mem.py の MemBuilder が C + アセンブリから
             libmem.a (自前アロケータ) を作る
  print/ ... print/build.py が C + アセンブリから
             libprint.a (println 相当) を作る
  regex/ ... regex/build.py が C + アセンブリから
             libregex.a (簡易正規表現エンジン) を作る

print と regex は Cargo (build.rs) からも呼ばれる build.py を
そのまま python3 で叩くことで、Rust を経由せずに単独でもビルドできる
ようにしている。
"""
import platform
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
MEM_DIR = ROOT / "mem"
PRINT_DIR = ROOT / "print"
REGEX_DIR = ROOT / "regex"

# mem/mem.py を `mem.mem` として import できるようにする
# (regex クレートが `regex/src/regex.rs` を `regex::regex` として
#  持っているのと同じ、パッケージ名とファイル名を揃える構成)
sys.path.insert(0, str(ROOT))
from mem.mem import MemBuilder


def build_mem() -> Path:
    print("=== [1/3] mem をビルド ===")
    lib_name = MemBuilder().build()
    return MEM_DIR / lib_name


def _run_build_py(build_dir: Path, extra_args=None) -> None:
    subprocess.run(
        [sys.executable, str(build_dir / "build.py"), *(extra_args or [])],
        cwd=build_dir,
        check=True,
    )


def build_print() -> Path:
    print("=== [2/3] print をビルド ===")
    _run_build_py(PRINT_DIR)
    return PRINT_DIR / "libprint.a"


def build_regex() -> Path:
    print("=== [3/3] regex をビルド ===")
    # regex/build.py はターゲットアーキテクチャ("x64" / "arm64")を
    # 第一引数に要求するので、実行環境から自動判定して渡す
    arch = "arm64" if platform.machine().lower() in ("arm64", "aarch64") else "x64"
    _run_build_py(REGEX_DIR, [arch])
    return REGEX_DIR / "libregex.a"


def main() -> None:
    libs = [build_mem(), build_print(), build_regex()]

    print()
    print("=== ビルド結果 ===")
    ok = True
    for lib in libs:
        exists = lib.exists()
        ok &= exists
        print(f"  {'OK ' if exists else 'NG '} {lib}")

    if not ok:
        print("一部のライブラリが生成されませんでした。")
        sys.exit(1)

    print("すべてのライブラリのビルドに成功しました。")


if __name__ == "__main__":
    main()
