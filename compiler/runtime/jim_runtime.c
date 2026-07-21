/* ==== jim runtime v0 ====
 * Embedded at the top of every generated translation unit. jimc evaluates the
 * #ifdef JIM_RT_* feature gates itself and emits only the blocks the program
 * reaches, so trivial programs carry a trivial runtime.
 * Memory model: bump arena, freed in one sweep at exit (jim has no free()).
 * Every rt_* function backs a compiler intrinsic (see docs/DESIGN.md section 6).
 */
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef JIM_RT_PANIC
#ifndef JIM_PANIC_ABORT
#include <setjmp.h> /* try/catch handler stack; omitted in panic=abort builds */
#endif
#endif
#ifdef JIM_RT_FLOATMATH
#include <math.h> /* transcendental float intrinsics only */
#endif
#ifdef JIM_RT_STRPARSE
#include <errno.h> /* @str_to_i64 / @str_to_f64 only */
#endif

#ifdef JIM_RT_STR
/* ---- strings (immutable byte views) ----
 * Emitted whenever the program touches a String in any way: a literal, a
 * String-typed declaration, or a runtime block whose helpers traffic in
 * jim_str. */
typedef struct {
    const char* ptr;
    int64_t len;
} jim_str;

static jim_str rt_str_lit(const char* p, int64_t n) {
    jim_str s;
    s.ptr = p;
    s.len = n;
    return s;
}
#endif /* JIM_RT_STR */

#ifdef JIM_RT_ALLOC
/* ---- arena ----
 * Emitted whenever anything allocates: a constructor, a container buffer, or
 * a runtime helper that builds a new string. Allocation-free programs carry
 * no allocator at all (and main skips rt_init/rt_shutdown). */

typedef struct rt_block {
    struct rt_block* next;
    size_t used;
    size_t cap;
} rt_block;

static rt_block* rt_arena = NULL;

static void rt_oom(void) {
    fputs("jim: out of memory\n", stderr);
    exit(1);
}

static void* rt_arena_alloc(size_t n) {
    n = (n + 7u) & ~(size_t)7u; /* 8-byte alignment; block payload starts 8-aligned */
    if (rt_arena == NULL || rt_arena->used + n > rt_arena->cap) {
        size_t cap = n > 65536 ? n : 65536;
        rt_block* b = (rt_block*)malloc(sizeof(rt_block) + cap);
        if (b == NULL) rt_oom();
        b->next = rt_arena;
        b->used = 0;
        b->cap = cap;
        rt_arena = b;
    }
    void* p = (char*)(rt_arena + 1) + rt_arena->used;
    rt_arena->used += n;
    return p;
}

static void rt_init(void) {}

static void rt_shutdown(void) {
    rt_block* b = rt_arena;
    while (b != NULL) {
        rt_block* next = b->next;
        free(b);
        b = next;
    }
    rt_arena = NULL;
}
#endif /* JIM_RT_ALLOC */

#ifdef JIM_RT_PANIC
/* ---- panics & try/catch ----
 * Panics unwind to the innermost try (setjmp/longjmp handler stack);
 * uncaught, they print and exit(1).
 *
 * In panic=abort builds (JIM_PANIC_ABORT - used by the browser playground,
 * where wasm setjmp/longjmp isn't portably available) there is no handler
 * stack: every panic prints and exits, and codegen omits try/catch handlers.
 */
#ifndef JIM_PANIC_ABORT
typedef struct rt_handler {
    jmp_buf buf;
    struct rt_handler* prev;
    int frame_top; /* stack-trace depth at try entry (longjmp restores it) */
} rt_handler;

static rt_handler* rt_handlers = NULL;
static jim_str rt_current_exc;
#endif
#endif

#ifdef JIM_RT_FRAMETOP
/* Shadow-stack depth: jimc defines JIM_RT_FRAMETOP whenever PANIC or DEBUG is
 * on (the try/catch handler restores it; debug frames maintain it). In a
 * release build with panics it stays 0 and only the handler touches it. */
static int rt_frame_top = 0;
#endif /* JIM_RT_FRAMETOP */

#ifdef JIM_RT_DEBUG
/* ---- stack traces ----
 * Debug builds (`jimc run`, `jimc build --debug`) maintain a shadow stack:
 * push/pop per jim function, plus a line store before each call. Release
 * builds emit none of these calls - rt_frame_top stays 0 and panics print
 * exactly as before. The frames beyond the cap are counted, not stored.
 */
typedef struct {
    const char* file;
    const char* fn;
    int32_t line;
} rt_frame;

#define RT_MAX_FRAMES 256
static rt_frame rt_frames[RT_MAX_FRAMES];

static void rt_push_frame(const char* file, const char* fn) {
    if (rt_frame_top < RT_MAX_FRAMES) {
        rt_frames[rt_frame_top].file = file;
        rt_frames[rt_frame_top].fn = fn;
        rt_frames[rt_frame_top].line = 0;
    }
    rt_frame_top++;
}

static void rt_pop_frame(void) {
    if (rt_frame_top > 0) rt_frame_top--;
}

static void rt_frame_line(int32_t line) {
    if (rt_frame_top > 0 && rt_frame_top <= RT_MAX_FRAMES) {
        rt_frames[rt_frame_top - 1].line = line;
    }
}

static void rt_print_trace(void) {
    int shown = rt_frame_top < RT_MAX_FRAMES ? rt_frame_top : RT_MAX_FRAMES;
    fputs("stack trace (most recent call first):\n", stderr);
    if (rt_frame_top > RT_MAX_FRAMES) {
        fprintf(stderr, "  ... %d deeper frame(s) not recorded\n", rt_frame_top - RT_MAX_FRAMES);
    }
    for (int i = shown - 1; i >= 0; i--) {
        fprintf(stderr, "  at %s (%s:%d)\n", rt_frames[i].fn, rt_frames[i].file, (int)rt_frames[i].line);
    }
}
#endif /* JIM_RT_DEBUG */

#ifdef JIM_RT_PANIC
static void rt_panic(jim_str msg) {
#ifndef JIM_PANIC_ABORT
    if (rt_handlers != NULL) {
        rt_current_exc = msg;
        rt_handler* h = rt_handlers;
        rt_handlers = h->prev;        /* pop before the jump */
        rt_frame_top = h->frame_top;  /* unwind the shadow stack too */
        longjmp(h->buf, 1);
    }
#endif
    fputs("jim panic: ", stderr);
    fwrite(msg.ptr, 1, (size_t)msg.len, stderr);
    fputc('\n', stderr);
#ifdef JIM_RT_DEBUG
    if (rt_frame_top > 0) rt_print_trace();
#endif
    exit(1);
}

static void rt_panic_cstr(const char* msg) {
    rt_panic(rt_str_lit(msg, (int64_t)strlen(msg)));
}

/* @panic from jim code carries its compile-time location. Caught panics keep
 * the bare message (an Exception's repr IS its message); only the fatal,
 * uncaught printout shows the location. In debug builds the shadow stack's
 * innermost frame already holds this location, so the full trace replaces
 * the single-line form. */
static void rt_panic_at(jim_str msg, const char* file, int line, const char* fn) {
#ifndef JIM_PANIC_ABORT
    if (rt_handlers != NULL) {
        rt_panic(msg);
    }
#endif
    fputs("jim panic: ", stderr);
    fwrite(msg.ptr, 1, (size_t)msg.len, stderr);
    fputc('\n', stderr);
#ifdef JIM_RT_DEBUG
    if (rt_frame_top > 0) {
        rt_print_trace();
        exit(1);
    }
#endif
    fprintf(stderr, "  at %s:%d (in %s)\n", file, line, fn);
    exit(1);
}

/* the message of the exception being handled (Exception's repr IS the message) */
static jim_str rt_exc_msg(jim_str e) { return e; }
#endif /* JIM_RT_PANIC */

#ifdef JIM_RT_OPT
/* optional reference types are nullable pointers; using None panics */
static void* rt_nonnull(void* p, const char* tyname) {
    if (p == NULL) {
        char buf[160];
        int n = snprintf(buf, sizeof buf, "used a None value where %s was needed", tyname);
        char* m = (char*)rt_arena_alloc((size_t)n);
        memcpy(m, buf, (size_t)n);
        rt_panic(rt_str_lit(m, n));
    }
    return p;
}

/* ---- optionals (T?) ----
 * A T? is a tagged value. Using a None where a value is needed panics
 * (later interceptable by try/catch).
 */

typedef struct { bool has; int64_t value; } jim_opt_i64;
typedef struct { bool has; double value; } jim_opt_f64;
typedef struct { bool has; bool value; } jim_opt_bool;
typedef struct { bool has; uint8_t value; } jim_opt_char;
typedef struct { bool has; jim_str value; } jim_opt_str;

#define JIM_DEFINE_OPT(sfx, T, tyname) \
    static jim_opt_##sfx rt_opt_##sfx##_some(T v) { \
        jim_opt_##sfx o; o.has = true; o.value = v; return o; \
    } \
    static jim_opt_##sfx rt_opt_##sfx##_none(void) { \
        jim_opt_##sfx o; memset(&o, 0, sizeof o); o.has = false; return o; \
    } \
    static T rt_opt_##sfx##_get(jim_opt_##sfx o) { \
        if (!o.has) rt_panic_cstr("used a None value where " tyname " was needed"); \
        return o.value; \
    } \
    static bool rt_opt_##sfx##_has(jim_opt_##sfx o) { return o.has; }

JIM_DEFINE_OPT(i64, int64_t, "Integer")
JIM_DEFINE_OPT(f64, double, "Float")
JIM_DEFINE_OPT(bool, bool, "Bool")
JIM_DEFINE_OPT(char, uint8_t, "Char")
JIM_DEFINE_OPT(str, jim_str, "String")
#endif /* JIM_RT_OPT */

#ifdef JIM_RT_BUF
/* ---- RawBuffer<T> ----
 * Unchecked raw storage for the std library's Array/Vector. Bounds checks
 * and growth logic live in jim code, not here. The compiler emits one
 * JIM_DEFINE_BUF(sfx, T) line per element type in use.
 */
#define JIM_DEFINE_BUF(sfx, T) \
    typedef struct { T* data; int64_t cap; } jim_buf_##sfx; \
    static jim_buf_##sfx jim_buf_##sfx##_alloc(int64_t n) { \
        jim_buf_##sfx b; \
        if (n < 0) n = 0; \
        b.data = (T*)rt_arena_alloc((size_t)n * sizeof(T)); \
        b.cap = n; \
        return b; \
    } \
    static T jim_buf_##sfx##_get(jim_buf_##sfx b, int64_t i) { return b.data[i]; } \
    static void jim_buf_##sfx##_set(jim_buf_##sfx b, int64_t i, T v) { b.data[i] = v; } \
    static int64_t jim_buf_##sfx##_capacity(jim_buf_##sfx b) { return b.cap; }
#endif /* JIM_RT_BUF */

#ifdef JIM_RT_INT
/* ---- Integer (checked 64-bit arithmetic) ---- */

static int64_t rt_i64_add(int64_t a, int64_t b) {
    int64_t r;
    if (__builtin_add_overflow(a, b, &r)) rt_panic_cstr("Integer overflow in addition");
    return r;
}

static int64_t rt_i64_sub(int64_t a, int64_t b) {
    int64_t r;
    if (__builtin_sub_overflow(a, b, &r)) rt_panic_cstr("Integer overflow in subtraction");
    return r;
}

static int64_t rt_i64_mul(int64_t a, int64_t b) {
    int64_t r;
    if (__builtin_mul_overflow(a, b, &r)) rt_panic_cstr("Integer overflow in multiplication");
    return r;
}

static int64_t rt_i64_divtrunc(int64_t a, int64_t b) {
    if (b == 0) rt_panic_cstr("Integer division by zero");
    if (a == INT64_MIN && b == -1) rt_panic_cstr("Integer overflow in division");
    return a / b;
}

static int64_t rt_i64_mod(int64_t a, int64_t b) {
    if (b == 0) rt_panic_cstr("Integer modulo by zero");
    if (a == INT64_MIN && b == -1) return 0;
    return a % b;
}

static int64_t rt_i64_neg(int64_t a) {
    if (a == INT64_MIN) rt_panic_cstr("Integer overflow in negation");
    return -a;
}

static bool rt_i64_eq(int64_t a, int64_t b) { return a == b; }
static bool rt_i64_lt(int64_t a, int64_t b) { return a < b; }
static double rt_i64_to_f64(int64_t a) { return (double)a; }

static jim_str rt_i64_to_string(int64_t v) {
    char buf[32];
    int n = snprintf(buf, sizeof buf, "%lld", (long long)v);
    char* p = (char*)rt_arena_alloc((size_t)n);
    memcpy(p, buf, (size_t)n);
    return rt_str_lit(p, n);
}
#endif /* JIM_RT_INT */

#ifdef JIM_RT_FLOAT
/* ---- Float ---- */

static double rt_f64_add(double a, double b) { return a + b; }
static double rt_f64_sub(double a, double b) { return a - b; }
static double rt_f64_mul(double a, double b) { return a * b; }
static double rt_f64_div(double a, double b) { return a / b; } /* IEEE: inf/nan */
static double rt_f64_neg(double a) { return -a; }
static bool rt_f64_eq(double a, double b) { return a == b; }
static bool rt_f64_lt(double a, double b) { return a < b; }

static int64_t rt_f64_to_i64(double v) {
    if (v != v) rt_panic_cstr("cannot convert NaN to Integer");
    if (v >= 9223372036854775808.0 || v < -9223372036854775808.0)
        rt_panic_cstr("Float out of Integer range");
    return (int64_t)v; /* truncates toward zero */
}

static jim_str rt_f64_to_string(double v) {
    char buf[64];
    int n = snprintf(buf, sizeof buf, "%.15g", v);
    /* make whole doubles read as Floats: 3 -> 3.0 */
    bool needs_point = true;
    for (int i = 0; i < n; i++) {
        char c = buf[i];
        if (c == '.' || c == 'e' || c == 'E' || c == 'n' || c == 'i') {
            needs_point = false;
            break;
        }
    }
    if (needs_point && n < (int)sizeof buf - 2) {
        buf[n++] = '.';
        buf[n++] = '0';
    }
    char* p = (char*)rt_arena_alloc((size_t)n);
    memcpy(p, buf, (size_t)n);
    return rt_str_lit(p, n);
}
#endif /* JIM_RT_FLOAT */

#ifdef JIM_RT_BOOLCHAR
/* ---- Bool / Char (a Char is one byte, 0-255) ---- */

static bool rt_bool_eq(bool a, bool b) { return a == b; }
static bool rt_char_eq(uint8_t a, uint8_t b) { return a == b; }
static bool rt_char_lt(uint8_t a, uint8_t b) { return a < b; }
static int64_t rt_char_to_i64(uint8_t c) { return (int64_t)c; }

static uint8_t rt_i64_to_char(int64_t v) {
    if (v < 0 || v > 255) rt_panic_cstr("Integer out of Char range (0-255)");
    return (uint8_t)v;
}

static jim_str rt_char_to_string(uint8_t c) {
    char* p = (char*)rt_arena_alloc(1);
    p[0] = (char)c;
    return rt_str_lit(p, 1);
}
#endif /* JIM_RT_BOOLCHAR */

#ifdef JIM_RT_STRING
/* ---- String ---- */

static int64_t rt_str_len(jim_str s) { return s.len; }

/* unchecked byte read - the std library's String.get adds the bounds check */
static uint8_t rt_str_byte(jim_str s, int64_t i) { return (uint8_t)s.ptr[i]; }

static jim_str rt_str_concat(jim_str a, jim_str b) {
    int64_t n = a.len + b.len;
    char* p = (char*)rt_arena_alloc((size_t)n);
    if (a.len > 0) memcpy(p, a.ptr, (size_t)a.len);
    if (b.len > 0) memcpy(p + a.len, b.ptr, (size_t)b.len);
    return rt_str_lit(p, n);
}

static bool rt_str_eq(jim_str a, jim_str b) {
    if (a.len != b.len) return false;
    if (a.len == 0) return true;
    return memcmp(a.ptr, b.ptr, (size_t)a.len) == 0;
}

static bool rt_str_lt(jim_str a, jim_str b) {
    int64_t n = a.len < b.len ? a.len : b.len;
    if (n > 0) {
        int c = memcmp(a.ptr, b.ptr, (size_t)n);
        if (c != 0) return c < 0;
    }
    return a.len < b.len;
}

/* zero-copy substring view - strings are immutable, so a slice can alias the
 * original bytes. Unchecked: the std library adds the bounds checks. */
static jim_str rt_str_slice(jim_str s, int64_t start, int64_t len) {
    return rt_str_lit(s.ptr + start, len);
}

/* one copy out of raw byte storage - the string-builder escape hatch */
static jim_str rt_str_from_bytes(const uint8_t* p, int64_t len) {
    if (len <= 0) return rt_str_lit("", 0);
    char* q = (char*)rt_arena_alloc((size_t)len);
    memcpy(q, p, (size_t)len);
    return rt_str_lit(q, len);
}
#endif /* JIM_RT_STRING */

#ifdef JIM_RT_STRPARSE
/* Strict decimal parse: optional sign, digits, nothing else. None otherwise
 * (including on overflow). */
static jim_opt_i64 rt_str_to_i64(jim_str s) {
    char buf[24]; /* sign + 19 digits + NUL fits every int64 */
    if (s.len <= 0 || s.len >= (int64_t)sizeof buf) return rt_opt_i64_none();
    char c0 = s.ptr[0];
    if (c0 != '+' && c0 != '-' && (c0 < '0' || c0 > '9')) return rt_opt_i64_none();
    memcpy(buf, s.ptr, (size_t)s.len);
    buf[s.len] = 0;
    errno = 0;
    char* end = NULL;
    long long v = strtoll(buf, &end, 10);
    if (errno != 0 || end != buf + s.len) return rt_opt_i64_none();
    return rt_opt_i64_some((int64_t)v);
}

/* Strict decimal/scientific parse ("2.5", "-1e9"). None otherwise. */
static jim_opt_f64 rt_str_to_f64(jim_str s) {
    char buf[64];
    if (s.len <= 0 || s.len >= (int64_t)sizeof buf) return rt_opt_f64_none();
    char c0 = s.ptr[0];
    if (c0 != '+' && c0 != '-' && c0 != '.' && (c0 < '0' || c0 > '9')) return rt_opt_f64_none();
    memcpy(buf, s.ptr, (size_t)s.len);
    buf[s.len] = 0;
    errno = 0;
    char* end = NULL;
    double v = strtod(buf, &end);
    if (errno != 0 || end != buf + s.len) return rt_opt_f64_none();
    return rt_opt_f64_some(v);
}
#endif /* JIM_RT_STRPARSE */

#ifdef JIM_RT_FLOATMATH
/* ---- Float math (libm; IEEE-permissive - domain errors yield nan/inf,
 * policy such as "None for sqrt of a negative" belongs in jim code) ---- */

static double rt_f64_sqrt(double x) { return sqrt(x); }
static double rt_f64_cbrt(double x) { return cbrt(x); }
static double rt_f64_hypot(double x, double y) { return hypot(x, y); }
static double rt_f64_exp(double x) { return exp(x); }
static double rt_f64_log(double x) { return log(x); }
static double rt_f64_log2(double x) { return log2(x); }
static double rt_f64_log10(double x) { return log10(x); }
static double rt_f64_sin(double x) { return sin(x); }
static double rt_f64_cos(double x) { return cos(x); }
static double rt_f64_tan(double x) { return tan(x); }
static double rt_f64_asin(double x) { return asin(x); }
static double rt_f64_acos(double x) { return acos(x); }
static double rt_f64_atan(double x) { return atan(x); }
static double rt_f64_atan2(double y, double x) { return atan2(y, x); }
static double rt_f64_fmod(double x, double y) { return fmod(x, y); }
static double rt_f64_pow(double x, double y) { return pow(x, y); }
static bool rt_f64_is_nan(double x) { return x != x; }
static bool rt_f64_is_inf(double x) { return isinf(x) != 0; }
static bool rt_f64_is_finite(double x) { return isfinite(x) != 0; }
#endif /* JIM_RT_FLOATMATH */

#ifdef JIM_RT_BITOPS
/* ---- Integer bit operations ---- */

static int64_t rt_i64_and(int64_t a, int64_t b) { return a & b; }
static int64_t rt_i64_or(int64_t a, int64_t b) { return a | b; }
static int64_t rt_i64_xor(int64_t a, int64_t b) { return a ^ b; }
static int64_t rt_i64_not(int64_t a) { return ~a; }

static int64_t rt_i64_shl(int64_t a, int64_t b) {
    if (b < 0 || b > 63) rt_panic_cstr("shift amount out of range (0-63)");
    return (int64_t)((uint64_t)a << (uint64_t)b); /* defined: overflow bits drop */
}

static int64_t rt_i64_shr(int64_t a, int64_t b) {
    if (b < 0 || b > 63) rt_panic_cstr("shift amount out of range (0-63)");
    return a >> b; /* arithmetic: the sign is preserved */
}
#endif /* JIM_RT_BITOPS */

#ifdef JIM_RT_IOPRINT
/* ---- io ---- */

static void rt_print_string(jim_str s) {
    fwrite(s.ptr, 1, (size_t)s.len, stdout);
    fputc('\n', stdout);
}

static void rt_print_err(jim_str s) {
    fwrite(s.ptr, 1, (size_t)s.len, stderr);
    fputc('\n', stderr);
}
#endif /* JIM_RT_IOPRINT */

#ifdef JIM_RT_IOFILE
/* file paths need NUL termination for the C library */
static char* rt_cstr(jim_str s) {
    char* p = (char*)rt_arena_alloc((size_t)s.len + 1);
    if (s.len > 0) memcpy(p, s.ptr, (size_t)s.len);
    p[s.len] = 0;
    return p;
}

/* one line from stdin without the newline (CRLF handled); None at EOF */
static jim_opt_str rt_read_line(void) {
    size_t cap = 128, n = 0;
    char* tmp = (char*)malloc(cap);
    if (tmp == NULL) rt_oom();
    int ch;
    while ((ch = fgetc(stdin)) != EOF && ch != '\n') {
        if (n == cap) {
            cap *= 2;
            char* t2 = (char*)realloc(tmp, cap);
            if (t2 == NULL) { free(tmp); rt_oom(); }
            tmp = t2;
        }
        tmp[n++] = (char)ch;
    }
    if (ch == EOF && n == 0) {
        free(tmp);
        return rt_opt_str_none();
    }
    if (n > 0 && tmp[n - 1] == '\r') n--; /* Windows CRLF */
    char* p = (char*)rt_arena_alloc(n);
    memcpy(p, tmp, n);
    free(tmp);
    return rt_opt_str_some(rt_str_lit(p, (int64_t)n));
}

static jim_opt_str rt_read_file(jim_str path) {
    FILE* f = fopen(rt_cstr(path), "rb");
    if (f == NULL) return rt_opt_str_none();
    if (fseek(f, 0, SEEK_END) != 0) { fclose(f); return rt_opt_str_none(); }
    long sz = ftell(f);
    if (sz < 0) { fclose(f); return rt_opt_str_none(); }
    if (fseek(f, 0, SEEK_SET) != 0) { fclose(f); return rt_opt_str_none(); }
    char* p = (char*)rt_arena_alloc((size_t)sz);
    size_t got = sz > 0 ? fread(p, 1, (size_t)sz, f) : 0;
    fclose(f);
    if (got != (size_t)sz) return rt_opt_str_none();
    return rt_opt_str_some(rt_str_lit(p, (int64_t)sz));
}

static jim_opt_i64 rt_file_write_mode(jim_str path, jim_str content, const char* mode) {
    FILE* f = fopen(rt_cstr(path), mode);
    if (f == NULL) return rt_opt_i64_none();
    size_t got = content.len > 0 ? fwrite(content.ptr, 1, (size_t)content.len, f) : 0;
    int close_err = fclose(f);
    if (got != (size_t)content.len || close_err != 0) return rt_opt_i64_none();
    return rt_opt_i64_some((int64_t)got);
}

static jim_opt_i64 rt_write_file(jim_str path, jim_str content) {
    return rt_file_write_mode(path, content, "wb");
}

static jim_opt_i64 rt_append_file(jim_str path, jim_str content) {
    return rt_file_write_mode(path, content, "ab");
}

static bool rt_file_exists(jim_str path) {
    FILE* f = fopen(rt_cstr(path), "rb");
    if (f != NULL) {
        fclose(f);
        return true;
    }
    return false;
}
#endif /* JIM_RT_IOFILE */
