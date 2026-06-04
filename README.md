# ChronoForth

Forth system for Commodore 64 with subroutine threading and tail-call elimination.

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg)](https://github.com/RichardLitt/standard-readme)
![License](https://img.shields.io/badge/License-Apache_2.0-blue)
![WASP v2.0.0](https://img.shields.io/badge/WASP-v2.0.0-blue)
![CDE v2.0.0](https://img.shields.io/badge/CDE-v2.0.0-green)
![MSS v2.0.0](https://img.shields.io/badge/MSS-v2.0.0-orange)

Minimal Forth for the C64, built on DurexForth. Stripped to a bare kernel with no graphics, no sound, no float. Subroutine-threaded with native-code optimizations: composite words are open-coded (MIN/MAX **4–7×**), the `DO…LOOP` back-edge is a direct `JMP` (**1.76× less loop overhead**), `DROP` compiles to a single `INX`, and **opt-in inline threading** (`+inline`) makes 30 hot primitives compile as native code instead of calls (1.29×–1.44× on stack-heavy code).

Every performance claim in this README is **machine-verified**: cycle counts come from a cycle-exact 6502 core ([chrono6502](https://github.com/chronomancy-io/chrono6502), fetched by `make emu`), cross-checked against VICE, and the full Forth-2012 test suite passes in-emulator. See [PERFORMANCE.md](PERFORMANCE.md).

## Background

### WASP Problem Definition

ChronoForth solves the **Workload-Aware Sufficient Placement (WASP)** problem for Forth interpretation on the 6502.

**Dataset**: Forth source text (words, definitions, control flow) plus C64 hardware state.

**Workload**: Q1 = execute a Forth program and compute stack/side effects, Q2 = resolve and execute words by name, Q3 = introspect interpreter state at any point.

**Encoding Tuple** `(k, E, I, T, {F_q})`:

| Symbol | ChronoForth Mapping |
|--------|---------------------|
| k = 4 | Data stack, Return stack, Dictionary, I/O state |
| E(r) | (data_stack[32], return_stack[128], dictionary[38KB], io_state[ports]) |
| I(c) | Split LSB/MSB zero-page stack (O(1)), linked-list dictionary (O(n)), hardware rstack |
| T(q) | Outer interpreter: PARSE-NAME → FIND-NAME → EXECUTE/COMPILE |
| {F_q} | Stack effect contracts: ( inputs -- outputs ) per word |

**WASP Guarantees** (each verified — see [PERFORMANCE.md](PERFORMANCE.md)):
1. **Sufficiency**: data_stack + return_stack + dictionary + io_state answers any query about execution state. *Evidence*: full Forth-2012 suite passes against this state model in `chrono6502 gate`.
2. **Exactness**: {F_q} stack effect contracts ensure each word produces exactly the documented outputs. *Evidence*: `chrono6502 selftest` checks 22 primitives' stack effects against their contracts; the standard suite checks the rest.
3. **Bounded Work**: Work(Q2) = O(n) dictionary search, O(1) stack ops; per-word work = a fixed, **measured** cycle count (DUP 24, SWAP 38, `+` 34, …), reduced to ~0 call overhead for the 30 inlined primitives.
4. **Minimality**: k = 4 — removing any dimension loses interpreter capability.

### CDE 4-Phase Pipeline

#### Phase 1: Workload Analysis
Three workload queries drive the design:
- **Q1**: "Given a Forth program and input, compute the resulting stack contents and side effects"
- **Q2**: "Resolve and execute words by name through the dictionary"
- **Q3**: "Introspect interpreter state at any point in execution"

#### Phase 2: Coordinate Encoding
Four dimensions encode interpreter state:
- **Data stack**: Ordered list of 16-bit values, split LSB/MSB in zero page (LSB $03–$3a, MSB $3b–$72)
- **Return stack**: 6502 hardware stack ($100-$1FF), call/return addresses
- **Dictionary**: Linked-list of word headers + subroutine-threaded code ($0801-$9FFF)
- **I/O state**: Memory-mapped ports, device registers, KERNAL state

#### Phase 3: Index Construction
- **Zero-page split stack**: LSB at $03–$3a, MSB at $3b–$72 — O(1) push/pop via the X register
- **Dictionary headers**: Linked-list growing downward from $9FFF, O(n) search via FIND-NAME
- **Hardware return stack**: Native 6502 JSR/RTS — zero overhead subroutine threading

#### Phase 4: Query Translation + Local Filtering
- **T(q)**: Outer interpreter loop: parse word → search dictionary → execute or compile
- **{F_q}**: Stack effect contracts `( inputs -- outputs )` enforce correctness per word
- **Tail-call elimination**: JSR+RTS → JMP for the last word in a definition (−6 cycles)
- **DO…LOOP back-edge**: `loop` compiles a direct `JMP` to the loop top and `(loop)` `RTS`es into it, replacing the ~39-cycle generic `BRANCH` (`forth/doloop.fs`) — ~33 cycles saved per iteration.
- **Inline threading** (opt-in): with `+inline`, the compiler emits the native body of 30 hot primitives in place of `JSR WORD`, removing the 12-cycle call/return overhead per occurrence (`asm/inline.asm`). Default off — inlined code can't be `SEE`-decompiled, so the default keeps decompilation working; `-inline` restores it.

### MSS Claim Classification

Bucket key: **D**efinition (true by construction), **G**uarantee (verified
claim), **A**ssumption (design bet), **U**nknown (open question). v2 attaches a
**reproducible** evidence handle to every D and G, and resolves the v1 Unknown.

| ID | Claim | Bucket | Evidence |
|----|-------|--------|----------|
| D1 | k = 4 dimensions (Data stack, Return stack, Dictionary, I/O) | Definition | ARCHITECTURE.md |
| D2 | Split LSB/MSB stack in zero page (`LSB=$3b`, `MSB=$73`) | Definition | `durexforth.asm`, `core.asm` |
| D3 | Subroutine threading via JSR/RTS | Definition | `compiler.asm` |
| D4 | 30 hot primitives inline as native code when compiling | Definition | `asm/inline.asm` |
| G1 | DUP executes in **24** cycles (v1 said 22) | Guarantee | `chrono6502 selftest` |
| G2 | SWAP executes in **38** cycles (v1 said 26) | Guarantee | `chrono6502 selftest` |
| G3 | Tail-call elimination replaces JSR+RTS with JMP | Guarantee | `control.asm` `EXIT`; suite passes |
| G4 | All core words follow their stack-effect contracts | Guarantee | full Forth-2012 suite (`chrono6502 gate`, 0 errors) |
| G5 | Inlining yields 1.29×–1.44× on stack-heavy definitions | Guarantee | `chrono6502 defcyc` OFF vs ON |
| G6 | Inlining causes **zero** turnkey size regression (system compiled compact) | Guarantee | boot "cart: 229 bytes remain" |
| G7 | Cycle counts match real hardware (VICE) | Guarantee | CIA-timer cross-check; page-cross reproduced |
| G8 | DO…LOOP back-edge is 1.76× cheaper (direct JMP vs generic BRANCH) | Guarantee | `chrono6502 defcyc` empty-loop 76218→43251 |
| A1 | 32-cell data stack sufficient for typical programs | Assumption | — |
| A2 | Linked-list dictionary search acceptable at C64 word counts | Assumption | — |
| ~~U1~~ | ~~Performance impact of very deep dictionary chains~~ → **resolved**: O(n) search is **compile/interpret-time only**; runtime of compiled code is unaffected (calls are direct addresses / inlined bodies) | was Unknown | PERFORMANCE.md §Complexity Proofs |

## Install

### Requirements

- ACME cross-assembler (v0.97+)
- VICE C64 emulator
- c1541 disk utility
- make

### Build

```bash
make chronoforth.d64
```

## Usage

```bash
x64sc chronoforth.d64
```

## Features

- Forth 2012 standard compatible (full core/core-ext/exception test suite passes)
- Subroutine-threaded execution with tail-call elimination
- **Inline threading** of 30 hot primitives, opt-in via `+inline`/`-inline` (default off; SEE-decompilable when off)
- Zero-page split LSB/MSB data stack
- Exception handling (CATCH/THROW)
- Bare kernel: no graphics, sound, or float compiled in (optional modules ship as loadable `.fs`)

## Architecture

| Component | Purpose |
|-----------|---------|
| `asm/core.asm` | Stack operations |
| `asm/interpreter.asm` | Word execution |
| `asm/compiler.asm` | Compilation |
| `asm/math.asm` | Arithmetic |
| `asm/io.asm` | Console/disk I/O |

See [ARCHITECTURE.md](ARCHITECTURE.md) for details.

## Performance

Verified core word timings (1 MHz 6502, callable word incl. RTS):

| Word | Cycles | Word | Cycles |
|------|-------:|------|-------:|
| DUP | 24 | `+` | 34 |
| SWAP | 38 | `-` | 34 |
| OVER | 24 | `1+` | 15 |

Inlining a primitive into a definition removes the ~12-cycle call overhead;
stack-heavy definitions run **1.29×–1.44×** faster. See
[PERFORMANCE.md](PERFORMANCE.md) for the full ledger, the optimization, and the
verification methodology.

## Documentation

- `manual/` - Complete reference manual (build with `make manual.pdf`)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## License

Apache-2.0 © 2026 Jacob Coleman — See [LICENSE](LICENSE) for details.
