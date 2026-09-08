import subprocess
import sys
from pathlib import Path

# 修正: この build.py は Cargo (build.rs) からも main.py からも、
# 素の `python3 build.py <arch>` としても呼ばれ得るが、そのどの経路でも
# `import uselib.mem` が解決できるとは限らない。
# uselib/ の親ディレクトリを毎回自前で sys.path に積むことで、
# 呼び出され方に関わらず "uselib" パッケージを解決できるようにする。
ROOT = Path(__file__).resolve().parent          # uselib/regex
USELIB_ROOT = ROOT.parent                       # uselib/
sys.path.insert(0, str(USELIB_ROOT.parent))     # uselib/ の親

import uselib.mem.mem as Mem

SRC_C = ROOT / "c"
BUILD = ROOT
SRC_A = ROOT / "asm" / "x64" if sys.argv[1] == "x64" else ROOT / "asm" / "arm64"
SRC_A = SRC_A / "linux"

BUILD.mkdir(exist_ok=True)

print("ROOT =", ROOT)
print("SRC  =", SRC_C)
print("BUILD =", BUILD)

file_lists = [list(SRC_C.rglob("*.c")), list(SRC_A.rglob("*.s"))]

[[print("  ", f) for f in files] for files in file_lists]

objects = []

# parser.c は malloc/free を使う(パターン文字列バッファの確保)ため、
# libc に頼らず mem/ の自前アロケータをリンクする。
mem_builder = Mem.MemBuilder()
mem_builder.build()
# 修正: mem_f_name (静的ライブラリ名 "libmem.a") をそのまま
# `ar rcs libregex.a *.o libmem.a` の様に渡すと、libmem.a は
# オブジェクトファイルではなくアーカイブそのものとして libregex.a の
# 1メンバーとして"入れ子"に格納されてしまう。ar/ld は入れ子アーカイブを
# 展開してシンボル解決してくれないため、mem 側のシンボル(malloc/free 等)
# が未解決のまま残ってしまう。static ライブラリごと埋め込むのではなく、
# mem がビルド済みの個々の .o を平らに(flat に)取り込む。
objects.extend(mem_builder.objects)

def files_com(file_list):
    for f in file_list:
        obj = BUILD / (f.stem + ".o")

        r=subprocess.run([
            "gcc",
            "-c",
            str(f),
            "-o",
            str(obj),
            "-O2",
            "-fPIC",
        ], check=True)
        print(" ll ", r)

        objects.append(obj)


def compile_c_program():
    [files_com(f) for f in file_lists]
    print("Objects:")
    [print("  ", obj) for obj in objects]
    if not objects:
        raise RuntimeError(
            f"No C or Assembly files found in {SRC_C}"
        )
    print("files *.o", objects)
    # static library
    subprocess.run([
        "ar",
        "rcs",
        str(BUILD / "libregex.a"),
        *map(str, objects),
    ], check=True)
    print("libregex.a created")

compile_c_program()