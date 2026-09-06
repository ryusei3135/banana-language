/*
 * println.c
 * ══════════════════════════════════════════════════════════════════════════
 * 外部ライブラリ不使用・インラインアセンブラ版 可変引数 println
 * 対応OS : Linux x86-64 / Windows x86-64
 *
 * コンパイル:
 *   Linux  : gcc  -O2 -Wall -Wextra -o demo  println.c
 *   Windows: x86_64-w64-mingw32-gcc -O2 -Wall -Wextra -o demo.exe println.c
 *
 * 使用ヘッダ:
 *   <stdarg.h> のみ。これはコンパイラ組み込みマクロの集合であり、
 *   libc / msvcrt へのリンクは一切発生しない。
 *
 * 対応フォーマット指定子:
 *   %d  %i          signed int      (修飾子: %ld, %lld)
 *   %u              unsigned int    (修飾子: %lu, %llu)
 *   %x  %X          unsigned hex    (修飾子: %lx, %llx …)
 *   %o              unsigned octal  (修飾子: %lo, %llo …)
 *   %s              const char*
 *   %c              char
 *   %p              void* (0x + 小文字 hex)
 *   %%              リテラル '%'
 * ══════════════════════════════════════════════════════════════════════════
 */

#include "arg.h"
#include "print.h"


/* ══════════════════════════════════════════════════════════════════════════
 *  SECTION 1 : プラットフォーム別 sys_write(buf, len)
 * ══════════════════════════════════════════════════════════════════════════ */

/* ┌────────────────────────────────────────────────────────────────────────
 * │ Windows 版
 * │  PEB (Process Environment Block) を GS セグメントレジスタ経由で取得し、
 * │  InLoadOrderModuleList を辿って kernel32.dll を特定。
 * │  PE エクスポートテーブルから GetStdHandle / WriteFile を動的に解決する。
 * │  外部シンボルへの依存ゼロ・インポートライブラリ不要。
 * └──────────────────────────────────────────────────────────────────────── */
#ifdef _WIN32

/*
 * pe_get_proc() ─ PE エクスポートテーブル線形探索
 *
 *  base : DLL のロードベースアドレス
 *  name : 検索する ASCII 関数名
 *  戻値 : 関数アドレス (見つからなければ NULL)
 *
 * PE32+ (x64) のオフセット:
 *   IMAGE_DOS_HEADER.e_lfanew            @ +0x3C
 *   DataDirectory[0].VirtualAddress      @ NT_HEADERS + 4(sig) + 20(FileHdr)
 *                                          + 112(OptHdr 内オフセット)
 *   IMAGE_EXPORT_DIRECTORY:
 *     NumberOfNames       @ +24
 *     AddressOfFunctions  @ +28
 *     AddressOfNames      @ +32
 *     AddressOfNameOrdinals @ +36
 */
static void *pe_get_proc(void *base, const char *name)
{
    u8  *b       = (u8 *)base;
    u32  pe_off  = *(u32 *)(b + 0x3C);       /* e_lfanew                   */
    u32  exp_rva = *(u32 *)(b + pe_off + 136);/* DataDirectory[0] RVA       */
    if (!exp_rva) return 0;

    u8  *exp   = b + exp_rva;
    u32  nn    = *(u32 *)(exp + 24);          /* NumberOfNames              */
    u32 *names = (u32 *)(b + *(u32 *)(exp + 32)); /* AddressOfNames RVA 配列 */
    u16 *ords  = (u16 *)(b + *(u32 *)(exp + 36)); /* AddressOfNameOrdinals  */
    u32 *funcs = (u32 *)(b + *(u32 *)(exp + 28)); /* AddressOfFunctions     */

    for (u32 i = 0; i < nn; i++) {
        const char *n = (const char *)(b + names[i]);
        const char *m = name;
        while (*m && *m == *n) { m++; n++; }
        if (!*m && !*n)
            return (void *)(b + funcs[ords[i]]);
    }
    return 0;
}

/*
 * find_kernel32() ─ PEB ウォークで kernel32.dll を検索
 *
 * x86-64 Windows のメモリレイアウト:
 *   GS:[0x60]                      → PEB*
 *   PEB           + 0x18           → Ldr (PEB_LDR_DATA*)
 *   PEB_LDR_DATA  + 0x10           → InLoadOrderModuleList (LIST_ENTRY)
 *
 * LDR_DATA_TABLE_ENTRY オフセット (x64):
 *   +0x00  InLoadOrderLinks  (LIST_ENTRY, 16 bytes)
 *   +0x10  InMemoryOrderLinks
 *   +0x20  InInitializationOrderLinks
 *   +0x30  DllBase           (void*, 8 bytes)
 *   +0x38  EntryPoint
 *   +0x40  SizeOfImage       (u32, 4 bytes)
 *   +0x44  Flags             (u32, 4 bytes)
 *   +0x48  FullDllName       (UNICODE_STRING, 16 bytes)
 *   +0x58  BaseDllName       (UNICODE_STRING)
 *            .Length  @ +0x58  (u16, UTF-16 バイト数)
 *            .Buffer  @ +0x60  (wchar_t*)
 */
static void *find_kernel32(void)
{
    void *peb;
    /* x86-64: GS セグメントオフセット 0x60 に PEB ポインタ */
    __asm__ volatile ("movq %%gs:0x60, %0" : "=r"(peb));

    /* PEB.Ldr → InLoadOrderModuleList を辿る */
    void *ldr  = *(void **)((u8 *)peb + 0x18);
    void *head = (u8 *)ldr  + 0x10;   /* LIST_ENTRY 番兵ノード           */
    void *cur  = *(void **)head;       /* Flink → 先頭モジュールエントリ */

    while (cur != head) {
        u16  namelen = *(u16  *)((u8 *)cur + 0x58); /* BaseDllName.Length  */
        u16 *namebuf = *(u16 **)((u8 *)cur + 0x60); /* BaseDllName.Buffer  */

        /*
         * "kernel32.dll" は 12 文字 = UTF-16LE で 24 バイト
         * 大文字小文字どちらにも対応 (Windows は大文字で格納することが多い)
         */
        if (namelen == 24 && namebuf) {
            static const u16 K32[12] = {
                'k','e','r','n','e','l','3','2','.','d','l','l'
            };
            int ok = 1;
            for (int i = 0; i < 12; i++) {
                u16 c = namebuf[i];
                if (c >= 'A' && c <= 'Z') c |= 0x20; /* 大文字→小文字 */
                if (c != K32[i]) { ok = 0; break; }
            }
            if (ok) return *(void **)((u8 *)cur + 0x30); /* DllBase */
        }
        cur = *(void **)cur;   /* Flink → 次のエントリ */
    }
    return 0;
}

/* 関数ポインタ型 */
typedef void *(*FP_GetStdHandle)(u32);
typedef int   (*FP_WriteFile)(void *, const void *, u32, u32 *, void *);

static void sys_write(const char *buf, int len)
{
    static FP_GetStdHandle gs_GetStdHandle;
    static FP_WriteFile    gs_WriteFile;

    /* 初回のみ kernel32 から関数アドレスを取得 */
    if (!gs_WriteFile) {
        void *k32     = find_kernel32();
        gs_GetStdHandle = (FP_GetStdHandle)pe_get_proc(k32, "GetStdHandle");
        gs_WriteFile    = (FP_WriteFile)   pe_get_proc(k32, "WriteFile");
    }

    /* STD_OUTPUT_HANDLE = (DWORD)(-11) = 0xFFFFFFF5 */
    void *hout = gs_GetStdHandle((u32)-11);
    u32   written;
    gs_WriteFile(hout, buf, (u32)len, &written, 0);
}

/* ┌────────────────────────────────────────────────────────────────────────
 * │ Linux x86-64 版
 * │  write(2) システムコールをアセンブリ命令 `syscall` で直接呼び出す。
 * │  libc の write() / puts() 等は一切使用しない。
 * │
 * │  System V AMD64 ABI (Linux syscall 規約):
 * │    rax = syscall 番号 (SYS_write = 1)
 * │    rdi = ファイルディスクリプタ (1 = stdout)
 * │    rsi = バッファポインタ
 * │    rdx = バイト数
 * │    syscall 実行後 rcx, r11 が破壊される
 * └──────────────────────────────────────────────────────────────────────── */
#else

static void sys_write(const char *buf, int len)
{
    linux_sys_write(buf, (long)len);
}

#endif /* _WIN32 */

/* ══════════════════════════════════════════════════════════════════════════
 *  SECTION 2 : 数値→文字列変換ユーティリティ
 * ══════════════════════════════════════════════════════════════════════════ */

/*
 * fmt_uint() ─ unsigned 64bit 整数 → 任意基数 ASCII 文字列
 *
 *  val  : 変換する値
 *  out  : 出力バッファ (最低 24 バイト確保のこと)
 *  base : 基数 (10 / 16 / 8 など)
 *  upper: 非0 なら A-F を大文字にする
 *  戻値 : 書き込んだ文字数
 */
static int fmt_uint(u64 val, char *out, unsigned base, int upper)
{
    static const char ldig[] = "0123456789abcdef";
    static const char udig[] = "0123456789ABCDEF";
    const char *d = upper ? udig : ldig;
    char  tmp[24];
    int   i = 0;

    if (!val) { out[0] = '0'; return 1; }
    while (val) { tmp[i++] = d[val % base]; val /= base; }

    /* 逆順に詰め替える */
    for (int j = 0; j < i; j++) out[j] = tmp[i - 1 - j];
    return i;
}

/*
 * fmt_sint() ─ signed 64bit 整数 → 10 進 ASCII 文字列
 *
 * LLONG_MIN の否定がオーバーフローしないよう
 * ~(u64)val + 1 (= 符号なし二の補数否定) を使用する。
 */
static int fmt_sint(i64 val, char *out)
{
    if (val < 0) {
        out[0] = '-';
        /* (u64)val は val+2^64。~(u64)val+1 = -val (mod 2^64) = |val| */
        return 1 + fmt_uint(~(u64)val + 1, out + 1, 10, 0);
    }
    return fmt_uint((u64)val, out, 10, 0);
}

/* ══════════════════════════════════════════════════════════════════════════
 *  SECTION 3 : println ── フォーマット付き可変引数出力 (末尾 \n 自動付加)
 * ══════════════════════════════════════════════════════════════════════════ */

#define PRINTLN_BUFSIZE 4096

void cprintln(const char *fmt, ...)
{
    char    out[PRINTLN_BUFSIZE];
    int     pos = 0;
    va_list ap;
    va_start(ap, fmt);

/* 1 文字書き込みマクロ。末尾 2 バイトは '\n' + 安全マージン用に確保 */
#define PUT(c) \
    do { if (pos < PRINTLN_BUFSIZE - 2) out[pos++] = (char)(c); } while (0)

/* 文字列ブロック書き込みマクロ */
#define WRITE(s, n) \
    do { \
        const char *_ws = (s); int _wn = (n); \
        while (_wn-- > 0 && pos < PRINTLN_BUFSIZE - 2) out[pos++] = *_ws++; \
    } while (0)

    while (*fmt) {
        /* '%' 以外はそのまま出力 */
        if (*fmt != '%') { PUT(*fmt++); continue; }

        fmt++;                      /* '%' を消費                  */
        if (!*fmt) break;           /* "%\0" の末端ガード          */

        /* ── 長さ修飾子 ('l' が 1 個か 2 個) ── */
        int lng = 0;
        while (*fmt == 'l') { lng++; fmt++; }
        if (!*fmt) break;

        char tmp[32];
        int  tlen;
        char spec = *fmt++;         /* 変換指定文字                */

        switch (spec) {

        /* ── 符号付き整数 (%d, %i, %ld, %lld) ── */
        case 'd': case 'i':
            if      (lng >= 2) tlen = fmt_sint((i64)va_arg(ap, long long), tmp);
            else if (lng == 1) tlen = fmt_sint((i64)va_arg(ap, long),      tmp);
            else               tlen = fmt_sint((i64)va_arg(ap, int),       tmp);
            WRITE(tmp, tlen);
            break;

        /* ── 符号なし整数 10 進 (%u, %lu, %llu) ── */
        case 'u':
            if      (lng >= 2) tlen = fmt_uint(va_arg(ap, unsigned long long), tmp, 10, 0);
            else if (lng == 1) tlen = fmt_uint(va_arg(ap, unsigned long),      tmp, 10, 0);
            else               tlen = fmt_uint(va_arg(ap, unsigned int),       tmp, 10, 0);
            WRITE(tmp, tlen);
            break;

        /* ── 16 進 (%x, %X, %lx, %llx …) ── */
        case 'x': case 'X': {
            int up = (spec == 'X');
            if      (lng >= 2) tlen = fmt_uint(va_arg(ap, unsigned long long), tmp, 16, up);
            else if (lng == 1) tlen = fmt_uint(va_arg(ap, unsigned long),      tmp, 16, up);
            else               tlen = fmt_uint(va_arg(ap, unsigned int),       tmp, 16, up);
            WRITE(tmp, tlen);
            break;
        }

        /* ── 8 進 (%o, %lo, %llo) ── */
        case 'o':
            if      (lng >= 2) tlen = fmt_uint(va_arg(ap, unsigned long long), tmp, 8, 0);
            else if (lng == 1) tlen = fmt_uint(va_arg(ap, unsigned long),      tmp, 8, 0);
            else               tlen = fmt_uint(va_arg(ap, unsigned int),        tmp, 8, 0);
            WRITE(tmp, tlen);
            break;

        /* ── ポインタ (%p → 0xXXXX…) ── */
        case 'p': {
            void *p = va_arg(ap, void *);
            uptr  v;
            /* ポインタ→整数: __builtin_memcpy で UB 回避 */
            __builtin_memcpy(&v, &p, sizeof v);
            PUT('0'); PUT('x');
            tlen = fmt_uint((u64)v, tmp, 16, 0);
            WRITE(tmp, tlen);
            break;
        }

        /* ── 文字列 (%s) ── */
        case 's': {
            const char *s = va_arg(ap, const char *);
            if (!s) s = "(null)";
            while (*s && pos < PRINTLN_BUFSIZE - 2) out[pos++] = *s++;
            break;
        }

        /* ── 文字 (%c) ── */
        case 'c':
            PUT(va_arg(ap, int));
            break;

        /* ── リテラル % (%%) ── */
        case '%':
            PUT('%');
            break;

        /* ── 未知の指定子 → そのまま出力 ── */
        default:
            PUT('%');
            for (int k = 0; k < lng; k++) PUT('l');
            PUT(spec);
            break;
        }
    }

    PUT('\n');          /* 末尾改行                         */
    va_end(ap);
    sys_write(out, pos);

#undef PUT
#undef WRITE
}

/*
int main(void)
{
    println("=== no-lib inline-asm println デモ ===");
    println("Hello, World!");

    println("%%d  signed int   : %d",   -42);
    println("%%u  unsigned     : %u",   99999u);
    println("%%x  hex lower    : 0x%x", 0xdeadbeefu);
    println("%%X  hex upper    : 0x%X", 0xCAFEBABEu);
    println("%%o  octal        : %o",   0755u);

    println("%%s  string       : %s",   "inline asm で動く");
    println("%%c  char         : %c",   'Z');

    println("%%ld  long        : %ld",  -1234567890L);
    println("%%lld long long   : %lld", -9999999999999LL);
    println("%%llu ull         : %llu", 18446744073709551615ULL);
    println("%%llx hex 64bit   : %llx", 0xDEADCAFEBABEBEEFULL);

    println("%%p  pointer      : %p",   (void *)main);

    println("100%%");

    println("%d * %d + %d = %d", 6, 7, 1, 43);

    println("LLONG_MIN        : %lld", (long long)-9223372036854775807LL - 1LL);

    return 0;
}*/