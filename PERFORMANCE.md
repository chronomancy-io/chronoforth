# Performance Analysis: ChronoForth

> **MSS v2.0.0** — every Guarantee below is backed by a reproducible
> measurement, not an estimate. Cycle counts are produced by a cycle-exact
> 6502 core (`tools/chrono6502`) and cross-checked against VICE on real C64
> timing. See [Methodology](#methodology--verification).

## Complexity Classification

### Core Word Cycle Counts (verified)

All counts are MOS 6502 cycles for the **callable word** (entry through its
final `RTS`, inclusive), interpreted path, measured by `chrono6502 selftest`.
They are exact: the 6502 is deterministic, the data stack lives entirely in
zero page (no page-cross on stack access), and the measurement is reproduced
bit-for-bit by the emulator and confirmed against VICE.

| Word | Cycles | Word | Cycles | Word | Cycles |
|------|-------:|------|-------:|------|-------:|
| DROP | 14 | `1+` | 15 | `=`  | 40 |
| DUP  | 24 | `1-` | 19 | `0=` | 30 |
| `?DUP`| 35 | `+`  | 34 | `<`  | 43 |
| SWAP | 38 | `-`  | 34 | `>`  | 43 |
| OVER | 24 | INVERT | 26 | `U<` | 42 |
| NIP  | 24 | NEGATE | 28 | MAX | 47 |
| TUCK | 56 | ROT  | 54 | MIN | 32 |
| `2DUP`| 42 | COUNT | 46 | | |

> Note: `DROP` is 14 cycles as a *called* word (`lda STATE / bne / inx / rts`);
> when **compiled** it emits a single `INX` (2 cycles, 1 byte) — `DROP` is
> immediate and inlines itself. See [Inline threading](#optimization-2-inline-threading).

### Correction notice (v1 → v2)

The v1.0.0 tables were hand-estimated and wrong. The verified values:

| Word | v1 claimed | **v2 measured** | v1 error |
|------|-----------:|----------------:|---------:|
| DUP  | 22 | **24** | −2, and ARCHITECTURE.md also said "6" |
| SWAP | 26 | **38** | −12 (−32%) |
| OVER | 22 | **24** | −2 |
| `+`  | 18 | **34** | −16 (−47%) |
| `-`  | 20 | **34** | −14 |
| `@`  | 18 | **24** | −6 |
| `!`  | 20 | **34** | −14 |

The split LSB/MSB zero-page stack is real and is what keeps these counts low,
but a 16-bit operation still touches both halves: e.g. `+` is
`lda/clc/adc/sta` on the low byte then `lda/adc/sta` on the high byte, `inx`,
`rts` = 4+2+4+4+4+4+4+2+6 = **34**, not 18.

## Complexity Proofs

### Dictionary lookup: O(n)
A linked list of headers searched newest-first by `FIND-NAME` (length compare,
then bytewise name compare). Worst case n comparisons. Hot words land near the
head only when defined late; kernel primitives are defined early and so sit deep
— this affects **compile/interpret time only**, never the runtime of already
compiled code.

### Compilation: O(1) amortized per word
Each compiled call is a 3-byte `JSR` (or, for an inlined primitive, a fixed-size
body copy); dictionary append is O(1); header creation is O(word length).

## Optimization 1: Open-coding composite primitives

Several core words were defined as chains of calls to other words, paying full
`JSR`/`RTS` overhead two to four times for an operation that fits in a dozen
straight-line instructions. Rewriting them as direct code (verified against the
Forth-2012 suite) gives large per-word wins:

| Word | was | now | speed-up | why it was slow |
|------|----:|----:|---------:|-----------------|
| MIN  | 225 | 32–47 | **5–7×** | `2dup > 0branch swap drop` |
| MAX  | 202 | 32–47 | **4–6×** | `2dup < 0branch swap drop` |
| WITHIN | ~200 | 92 | **~2.2×** | `over - >r - r> u<` |
| `>`  | 90  | 43  | **2.1×** | `swap <` |
| COUNT| ~100| 46  | **~2.2×**| `dup 1+ swap c@` |
| NIP  | 52  | 24  | **2.2×** | `swap drop` |
| NEGATE| 50 | 28  | **1.8×** | `invert 1+` |
| `<>` | ~70 | 40  | **1.75×** | `= 0=` (was a Forth word) |
| `0<>`| ~60 | 27  | **2.2×** | `0= 0=` (was a Forth word) |
| `u>` | ~58 | 41  | 1.4× | `swap u<` (was a Forth word) |
| S>D  | ~40 | 25  | 1.6× | `dup 0<` |
| `2DUP`| 57 | 42  | 1.36× | `over over` |
| TUCK | 71  | 56  | 1.27× | `swap over` |

MIN/MAX are now a single signed 16-bit compare plus a conditional cell move
instead of a dup-compare-branch-swap dance. The effect compounds: the kernel
itself uses these words, so the full test-suite run got ~5 M cycles faster
(208.5 M → 203.7 M). `<>`/`0<>`/`u>` were promoted from Forth definitions in
`base.fs` to native kernel words.

**Constant pushers** `0` `1` `-1` `BL` were `jsr … jmp pushya` (~29 cycles for a
compiled `0`); open-coded to a direct push (~18) and inlined, a compiled `0`
now costs ~12 cycles. These are some of the most frequent words in real code.

All of these open-coded words are straight-line, so they join the inline set.

## Optimization 2: Inline threading

Subroutine threading's structural cost is the call itself: `JSR` (6) + `RTS`
(6) = **12 cycles per word**, plus 3 bytes. For a 24-cycle word like `DUP`
that is 50% overhead.

ChronoForth's compiler (`asm/inline.asm`) inlines the machine-code body of **30
hot primitives** directly into a definition instead of calling them:

```
DUP DROP OVER SWAP ROT NIP TUCK 2DUP  +  -  1+ 1- NEGATE INVERT  0 1 -1 BL
=  0=  <  >  @  !  2*  MAX MIN
```

(`DROP` self-inlines to `INX`; the rest copy their position-independent bodies.)

### Measured speed-ups (cycle-exact, inline OFF vs ON)

| Definition | OFF | ON | saved | speed-up |
|------------|----:|---:|------:|---------:|
| `: q dup + ;`                 | 67  | 52  | 15 | **1.29×** |
| `: s over + swap - ;`         | 151 | 112 | 39 | **1.35×** |
| `: r dup + dup + dup + ;`     | 207 | 144 | 63 | **1.44×** |

Each inlined occurrence removes the ~12-cycle call/return overhead. The win
scales with how primitive-dense the definition is.

### Opt-in (default off), zero size-regression

Inlining everything — including the system source (`base.fs`, the assembler,
the editor) — overflowed the cartridge ROM budget by ~4.8 KB, so the kernel
compiles the system with inlining **off** (cartridge still fits, "229 bytes
remain"). Inlining also trades away source-level decompilation: a word built
from inlined machine code can't be reconstructed by `SEE`. So it stays a
**default-off, opt-in toggle** (`inlining_on`): turn it on for hot code with
`+inline` (and `-inline` to restore). `SEE` works on any word compiled while
inlining is off; the open-coding (Optimization 1) and the loop back-edge
(Optimization 4) are unconditional native wins regardless.

Safety contract for the inline table: a word may be inlined only if its body
ends in a single `RTS`, contains no `JSR`/`JMP` and no inline operands (only
self-relative branches), so the copied bytes are position-independent. Inlined
words are flagged no-TCE so tail-call elimination never rewrites a copied body.

## Optimization 3: Tail-call elimination (pre-existing)

`EXIT` rewrites the last compiled `JSR WORD` into `JMP WORD`, dropping the
trailing `RTS` (saves 6 cycles + reuses the callee's return). Composes with
inlining: a definition ending in an inlined primitive is exempted from TCE and
simply gets its `RTS` appended.

## Considered and rejected

- **Dictionary reordering** (move hot words to the search head): benefits
  compile/interpret time only, not compiled-code runtime, and risks breaking
  definition-order dependencies in the kernel. Not worth the risk.
- **Inlining `C@`/`C!`**: they self-modify an *absolute* operand address, so a
  copied body would patch the original word, not the copy. Unsafe — excluded.
- **Inlining words that tail-`JMP` or read inline operands** (e.g. `MAX`/`MIN`
  *before* they were open-coded, or `LIT`/`BRANCH`): the internal `jmp` or the
  operand fetch off the return address would break a copied body. The fix where
  it paid off was to *open-code* the word (Optimization 1) so it ends in a plain
  `RTS` and becomes inlinable; words that can't be are left as calls.
- **Optimizing compile-time words** (`IF`/`THEN`/`WHILE`/`REPEAT`/`PARSE-NAME`):
  they run during compilation, not in compiled code, so this speeds builds, not
  runtime — deliberately out of scope.

## Optimization 4: DO…LOOP back-edge

The native `(loop)` increments the index and, when not done, used to loop back
via the generic `BRANCH` word — ~39 cycles to read the branch target indirectly
*every* iteration — so an empty loop cost ~76 cycles/iteration. `loop` now
compiles `jsr (loop) ; jmp <do-target>` and `(loop)` simply `RTS`es into that
`jmp` on the not-done path. The back-edge drops to ~12 cycles.

| Benchmark | before | after | speed-up |
|-----------|-------:|------:|---------:|
| empty 1000-iter loop      | 76218 | 43251 | **1.76×** loop overhead |
| `: w 0 300 0 do 1+ loop drop ;` | 25689 | 15822 | **1.62×** |

≈ 33 cycles saved per iteration — and loops are the hottest construct in real
programs, so this compounds with the inlined body of every loop.

The subtle part was the compiler interaction (found with `chrono6502 dis`/`dict`
+ a kernel-code write-watch). A first attempt used the `jmp,` token to emit the
opcode, but at `doloop.fs` compile time `jmp,` resolves to the **assembler**'s
`( addr -- )` definition (`$4c 3mi`), which *consumes* the do-target off the
stack — starving `resolve-leaves` of its `( ?dopos dopos )` argument so its
leave-stack walk underflowed and wrote `HERE` into kernel code (`pushya` at
`$0821`). The fix emits the opcode byte directly with `$4c c,`, leaving the
existing `dup ,` to compile the target and preserving the stack contract. All of
`leave` / `?do` / `+loop` / `unloop` / `i` / `j` and the loop frame are
unchanged and still pass the full suite.

## Methodology & Verification

Performance and correctness are measured by `tools/chrono6502`, a dependency-free
cycle-exact NMOS 6502 core in Rust:

- **`selftest` / `ledger`** — JSR a word by its ACME symbol address, count cycles
  to `RTS`, verify the stack effect against its `( -- )` contract.
- **`defcyc`** — boot the real Forth, compile a definition, JSR-measure it on the
  post-boot snapshot (used for the inline OFF/ON numbers above).
- **`gate`** — run the entire Forth-2012 test suite
  (`tester`+`testcore`+`testcoreplus`+`testcoreext`+`testexception`) in-process
  in ~0.4 s; exit 0 ⇔ 0 errors. Used as the correctness gate for every change.

The core is validated four independent ways:

1. **Exact vs hand-derivation** on 22 straight-line primitives (`selftest`).
2. **11 `cargo test` unit tests** of the cycle model — page-cross penalties,
   branch taken/not-taken/cross, ADC/SBC flags, zero-page-X wrap, JSR/RTS.
3. **Full Forth-2012 suite passes** in-emulator (functional faithfulness).
4. **VICE cross-check** — a deterministic loop measured in real VICE via a CIA
   #2 Φ2 timer (screen blanked, IRQ off ⇒ Φ2 = pure CPU). The emulator
   reproduces VICE's count, *including* a placement-dependent +1-cycle/iteration
   page-cross on an inlined branch (`bne` straddling `$xx00`): the emulator
   shows the identical +299/300-iteration jump when the word is relocated across
   a page boundary.

### Reproduce

```bash
make durexforth.prg
acme -I asm --vicelabels labels.vice asm/durexforth.asm
cd tools/chrono6502 && cargo build --release && cargo test
./target/release/chrono6502 --prg ../../durexforth.prg --labels ../../labels.vice selftest
./target/release/chrono6502 --prg ../../durexforth.prg gate
./target/release/chrono6502 --prg ../../durexforth.prg defcyc ": r dup + dup + dup + ;" r 5
```

## Memory & threading model (unchanged)

| Model | Call overhead | Inlinable | Size |
|-------|--------------:|-----------|------|
| Direct threading | 12 | no | small |
| Indirect threading | 18 | no | smallest |
| **Subroutine (ChronoForth)** | **12, or 0 when inlined** | **yes** | medium |
| Native code | 0 | n/a | large |

Subroutine threading + selective inlining gives native-code speed on the hot
path while keeping the rest compact.
