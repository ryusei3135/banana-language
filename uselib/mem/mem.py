import subprocess
import platform
from pathlib import Path

ROOT = Path(__file__).resolve().parent
# 修正: BUILD はファイルではなくディレクトリ(mem/ 直下)にする。
# 元コードは `Path.cwd() / "mem.o"` という「1個のファイル」を BUILD にしていて、
# そこに `self.BUILD / (f.stem + ".o")` のようにサブパスを作ろうとしたり
# (BUILD がディレクトリでないと存在しえない)、存在しない self.BUILD 属性を
# 参照したりしていて、実行すると必ず落ちる状態だった。
BUILD = ROOT
LIB_NAME = "libmem.a"

# 修正: linux/*.s しか見ておらず Windows 未対応だった点も、
# print/regex の build.py と同様に OS で asm ディレクトリを切り替えるようにした。
SRC_A = ROOT / ("win" if platform.system() == "Windows" else "linux")


class MemBuilder:
    def __init__(self):
        BUILD.mkdir(exist_ok=True)
        self.file_lists = [
            [f for f in ROOT.glob("*.c")],
            list(SRC_A.rglob("*.s")),
            list(SRC_A.rglob("*.c")),
        ]
        [[print("  ", f) for f in files] for files in self.file_lists]
        self.objects = []

    def files_com(self, file_list):
        for f in file_list:
            obj = BUILD / (f.stem + ".o")
            subprocess.run([
                "gcc",
                "-c",
                str(f),
                "-o",
                str(obj),
                "-O2",
                "-fPIC",
            ], check=True)
            self.objects.append(obj)

    # ビルドしたライブラリのファイル名を返す
    def build(self) -> str:
        [self.files_com(f) for f in self.file_lists]
        if not self.objects:
            raise RuntimeError(
                f"No C or Assembly files found in {ROOT}"
            )
        print("files *.o", self.objects)

        # 修正: mem は main() を持たないライブラリなので、実行ファイルではなく
        # print/regex と同様に静的ライブラリ (libmem.a) としてまとめる。
        subprocess.run([
            "ar",
            "rcs",
            str(BUILD / LIB_NAME),
            *map(str, self.objects),
        ], check=True)

        print(f"{LIB_NAME} created")
        return LIB_NAME