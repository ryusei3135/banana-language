#!/usr/bin/env bash
#
# add_c_highlight.sh
#
# .nvim.lua (lazy.nvim) に、C言語ハイライト用の設定を"追記"するスクリプト。
#
#   1. nvim-treesitter プラグインを追加し、Cパーサをインストール・有効化する
#      (既存のプラグイン一覧はそのまま。行を追加するだけで削除・変更はしない)
#   2. treesitter が使えない/Cパーサのビルドに失敗した場合のフォールバックとして、
#      classic syntax (c.vim) 用の cXxx ハイライトグループを、既存の rustXxx /
#      @-group と同じ配色で追加する
#
# 既存の @keyword / @type / @function / @string / @comment などの
# treesitter 汎用グループは言語非依存なので、Cパーサが入った時点で
# 実質そのまま Rust と同じ配色が C にも適用されます。
# このスクリプトはあくまで「Cパーサを有効にする」「保険として classic
# グループも埋める」の2点だけを行い、既存のプラグイン定義やハイライト定義には
# 一切触れません（追記のみ・冪等）。
#
# 使い方:
#   ./add_c_highlight.sh [path/to/.nvim.lua]
#   (省略時はカレントディレクトリの .nvim.lua を対象にします)
#
set -euo pipefail

TARGET="${1:-.nvim.lua}"

PLUGIN_MARK_BEGIN="    -- >>> add_c_highlight.sh: treesitter(c) auto-added, do not edit between markers >>>"
PLUGIN_MARK_END="    -- <<< add_c_highlight.sh: treesitter(c) <<<"
HL_MARK_BEGIN="    -- >>> add_c_highlight.sh: c-highlight fallback auto-added, do not edit between markers >>>"
HL_MARK_END="    -- <<< add_c_highlight.sh: c-highlight fallback <<<"

if [ ! -f "$TARGET" ]; then
    echo "error: '$TARGET' が見つかりません" >&2
    exit 1
fi

if ! grep -q 'require("lazy")\.setup(' "$TARGET"; then
    echo "error: '$TARGET' に require(\"lazy\").setup({ ... }) が見つかりません" >&2
    echo "       (lazy.nvim を使っていない設定ファイルには対応していません)" >&2
    exit 1
fi

if ! grep -q 'local function apply_highlights()' "$TARGET"; then
    echo "error: '$TARGET' に 'local function apply_highlights()' が見つかりません" >&2
    exit 1
fi

BACKUP="${TARGET}.bak.$(date +%Y%m%d%H%M%S)"
cp "$TARGET" "$BACKUP"
echo "backup: $BACKUP"

TMP="$(mktemp)"
trap 'rm -f "$TMP" "$TMP.2" "$TMP.3" "$TMP.4"' EXIT

# 1) 以前このスクリプトで挿入したブロックが残っていれば、いったん取り除く
#    (何度実行しても重複追記されないようにするため)
awk -v b="$PLUGIN_MARK_BEGIN" -v e="$PLUGIN_MARK_END" '
    $0==b {skip=1; next}
    $0==e {skip=0; next}
    skip!=1 {print}
' "$TARGET" > "$TMP"

awk -v b="$HL_MARK_BEGIN" -v e="$HL_MARK_END" '
    $0==b {skip=1; next}
    $0==e {skip=0; next}
    skip!=1 {print}
' "$TMP" > "$TMP.2"

# 2) require("lazy").setup({ の直後に nvim-treesitter プラグインを追加
#    -> 既存のプラグインエントリ(nvim-lspconfig, vscode.nvim, ...)には触れない
awk -v b="$PLUGIN_MARK_BEGIN" -v e="$PLUGIN_MARK_END" '
    {
        print
        if (!done && $0 ~ /require\("lazy"\)\.setup\(\{/) {
            print b
            print "    {"
            print "        \"nvim-treesitter/nvim-treesitter\","
            print "        build = \":TSUpdate\","
            print "        opts = {"
            print "            -- 既に他の設定で ensure_installed / highlight を"
            print "            -- 指定している場合はそちらとマージされます"
            print "            ensure_installed = { \"c\" },"
            print "            highlight = { enable = true },"
            print "        },"
            print "    },"
            print e
            done=1
        }
    }
' "$TMP.2" > "$TMP.3"

# 3) local function apply_highlights() の直後に、C言語用の classic syntax
#    フォールバックグループを追加 (.nvim.lua の rustXxx / @-group と同じ配色)
awk -v b="$HL_MARK_BEGIN" -v e="$HL_MARK_END" '
    {
        print
        if (!done && $0 ~ /local function apply_highlights\(\)/) {
            print b
            print "    -- C言語 (classic syntax / c.vim のフォールバック。"
            print "    -- treesitterのCパーサが有効な場合は @keyword 等の"
            print "    -- 汎用グループが優先されるため見た目は基本的に同じになる)"
            print "    vim.api.nvim_set_hl(0, \"cStatement\",       { fg = \"#6CFF6C\", bold = true })"
            print "    vim.api.nvim_set_hl(0, \"cLabel\",           { fg = \"#6CFF6C\", bold = true })"
            print "    vim.api.nvim_set_hl(0, \"cConditional\",     { fg = \"#6CFF6C\", bold = true })"
            print "    vim.api.nvim_set_hl(0, \"cRepeat\",          { fg = \"#6CFF6C\", bold = true })"
            print ""
            print "    vim.api.nvim_set_hl(0, \"cStorageClass\",    { fg = \"#8EF7B5\", bold = true })"
            print "    vim.api.nvim_set_hl(0, \"cTypedef\",         { fg = \"#8EF7B5\", bold = true })"
            print "    vim.api.nvim_set_hl(0, \"cStructure\",       { fg = \"#2ECC71\", bold = true })"
            print "    vim.api.nvim_set_hl(0, \"cType\",            { fg = \"#2ECC71\", bold = true })"
            print ""
            print "    vim.api.nvim_set_hl(0, \"cBoolean\",         { fg = \"#D6FF6B\", bold = true })"
            print "    vim.api.nvim_set_hl(0, \"cConstant\",        { fg = \"#D6FF6B\" })"
            print "    vim.api.nvim_set_hl(0, \"cNumber\",          { fg = \"#D6FF6B\" })"
            print "    vim.api.nvim_set_hl(0, \"cFloat\",           { fg = \"#D6FF6B\" })"
            print "    vim.api.nvim_set_hl(0, \"cOctal\",           { fg = \"#D6FF6B\" })"
            print ""
            print "    vim.api.nvim_set_hl(0, \"cString\",          { fg = \"#B7F5A0\" })"
            print "    vim.api.nvim_set_hl(0, \"cCppString\",       { fg = \"#B7F5A0\" })"
            print "    vim.api.nvim_set_hl(0, \"cCharacter\",       { fg = \"#B7F5A0\" })"
            print "    vim.api.nvim_set_hl(0, \"cSpecial\",         { fg = \"#7CE495\", bold = true })"
            print "    vim.api.nvim_set_hl(0, \"cSpecialCharacter\",{ fg = \"#7CE495\", bold = true })"
            print "    vim.api.nvim_set_hl(0, \"cFormat\",          { fg = \"#7CE495\", bold = true })"
            print ""
            print "    vim.api.nvim_set_hl(0, \"cComment\",         { fg = \"#1F5C2E\", italic = true })"
            print "    vim.api.nvim_set_hl(0, \"cCommentL\",        { fg = \"#1F5C2E\", italic = true })"
            print "    vim.api.nvim_set_hl(0, \"cCommentStart\",    { fg = \"#1F5C2E\", italic = true })"
            print "    vim.api.nvim_set_hl(0, \"cTodo\",            { fg = \"#5AC8A8\", italic = true })"
            print ""
            print "    vim.api.nvim_set_hl(0, \"cOperator\",        { fg = \"#5AA5E8\" })"
            print ""
            print "    -- プリプロセッサ (#include はモジュール読み込みっぽい色、"
            print "    -- #define はマクロっぽい色、に寄せている)"
            print "    vim.api.nvim_set_hl(0, \"cInclude\",         { fg = \"#0F8A3B\", bold = true })"
            print "    vim.api.nvim_set_hl(0, \"cPreCondit\",       { fg = \"#0F8A3B\", bold = true })"
            print "    vim.api.nvim_set_hl(0, \"cIncluded\",        { fg = \"#B7F5A0\" })"
            print "    vim.api.nvim_set_hl(0, \"cDefine\",          { fg = \"#A8FF60\", bold = true })"
            print "    vim.api.nvim_set_hl(0, \"cPreProc\",         { fg = \"#A8FF60\", bold = true })"
            print e
            done=1
        }
    }
' "$TMP.3" > "$TMP.4"

mv "$TMP.4" "$TARGET"
echo "updated: $TARGET"
echo ""
echo "確認: nvim を再起動するか :source ${TARGET} 、その後 :TSUpdate c を実行してください"
