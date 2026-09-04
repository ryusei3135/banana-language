// AArch64 (ARM64) 版
// 元の x86-64 (AT&T構文) からの移植。データ部はそのまま、
// レジスタ / アドレッシング / 分岐命令だけを AArch64 用に置き換えている。

.data
.balign 4                     // 4バイトアライメントに揃える (AArch64の .align は 2^n 指定なので .balign を使う)
empty:
    .byte 255, 255, 255
.balign 4
num:
    .byte 1
    .byte '0', '9'
.balign 4
alpha:
    .byte 4
    .byte 'a', 'z'
    .byte 'A', 'Z'
    .byte '0', '9'
    .byte '_', '_'
.balign 4
space:
    .byte 4
    .byte ' ', ' '
    .byte '\t', '\t'
    .byte '\n', '\n'
    .byte '\r', '\r'

.text
.global shorthand_class_ranges
.type shorthand_class_ranges, %function   // AArch64 の GAS では '@' は行コメント記号なので %function を使う

shorthand_class_ranges:
    // AArch64 の呼び出し規約では第1引数は w0/x0 (x86-64 の %dil/%rdi に相当)
    and     w1, w0, #0xff        // movzbl %dil, %ecx に相当 (下位8bitをゼロ拡張)

    cmp     w1, #'d'
    b.ne    .N0
    adrp    x0, num              // シンボルを含むページのアドレスを取得
    add     x0, x0, :lo12:num    // ページ内オフセットを加算して実アドレスに
    ret
.N0:
    cmp     w1, #'w'
    b.ne    .N1
    adrp    x0, alpha
    add     x0, x0, :lo12:alpha
    ret
.N1:
    cmp     w1, #'s'
    b.ne    .N2
    adrp    x0, space
    add     x0, x0, :lo12:space
    ret
.N2:
    adrp    x0, empty
    add     x0, x0, :lo12:empty
    ret
