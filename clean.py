# .oなどのファイルを消す

import os
import subprocess

def delete_line(path):
    for name in os.listdir(path):
        full_path = os.path.join(path, name)
        if os.path.isdir(full_path):
            delete_line(full_path)  # 再帰して合計
        else:
            f_name = f"{path}/{name}"
            if ".o" in f_name or ".a" in f_name:
                print(f"delete {f_name}")
                subprocess.run([
                    "rm",
                    "-r",
                    f_name
                ], check=True)
# 例
if __name__ == "__main__":
    delete_line("uselib")
