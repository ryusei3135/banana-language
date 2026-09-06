import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).parent
SRC_C = ROOT / "c"
BUILD = ROOT
SRC_A = ROOT / "asm" / "x64" if sys.argv[1] == "x64" else ROOT / "asm" / "arm64"

BUILD.mkdir(exist_ok=True)

print("ROOT =", ROOT)
print("SRC  =", SRC_C)
print("BUILD =", BUILD)

file_lists = [list(SRC_C.rglob("*.c")), list(SRC_A.rglob("*.s"))]

[[print("  ", f) for f in files] for files in file_lists]

objects = []

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