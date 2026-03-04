# ChronoForth

Forth system for Commodore 64 with subroutine threading and tail-call elimination.

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg)](https://github.com/RichardLitt/standard-readme)
![License](https://img.shields.io/badge/License-Apache_2.0-blue)
![WASP v1.0.0](https://img.shields.io/badge/WASP-v1.0.0-blue)
![CDE v1.0.0](https://img.shields.io/badge/CDE-v1.0.0-green)
![MSS v1.0.0](https://img.shields.io/badge/MSS-v1.0.0-orange)

Minimal Forth for the C64, built on DurexForth. Stripped to a bare kernel with no graphics, no sound, no float. Subroutine-threaded with tail-call elimination; DROP compiles to a single INX (2 cycles, 1 byte).

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

**WASP Guarantees**:
1. **Sufficiency**: data_stack + return_stack + dictionary + io_state answers any query about execution state
2. **Exactness**: {F_q} stack effect contracts ensure each word produces exactly the documented outputs
3. **Bounded Work**: Work(Q2) = O(n) dictionary search, O(1) stack ops; Work per word = constant cycles (see Performance)
4. **Minimality**: k = 4 — removing any dimension loses interpreter capability

### CDE 4-Phase Pipeline

#### Phase 1: Workload Analysis
Three workload queries drive the design:
- **Q1**: "Given a Forth program and input, compute the resulting stack contents and side effects"
- **Q2**: "Resolve and execute words by name through the dictionary"
- **Q3**: "Introspect interpreter state at any point in execution"

#### Phase 2: Coordinate Encoding
Four dimensions encode interpreter state:
- **Data stack**: Ordered list of 16-bit values, split LSB/MSB in zero page ($02-$41)
- **Return stack**: 6502 hardware stack ($100-$1FF), call/return addresses
- **Dictionary**: Linked-list of word headers + subroutine-threaded code ($0801-$9FFF)
- **I/O state**: Memory-mapped ports, device registers, KERNAL state

#### Phase 3: Index Construction
- **Zero-page split stack**: LSB at $02-$21, MSB at $22-$41 — O(1) push/pop via X register
- **Dictionary headers**: Linked-list growing downward from $9FFF, O(n) search via FIND-NAME
- **Hardware return stack**: Native 6502 JSR/RTS — zero overhead subroutine threading

#### Phase 4: Query Translation + Local Filtering
- **T(q)**: Outer interpreter loop: parse word → search dictionary → execute or compile
- **{F_q}**: Stack effect contracts `( inputs -- outputs )` enforce correctness per word
- Tail-call elimination: JSR+RTS → JMP reduces overhead for last word in definitions

### MSS Claim Classification

| ID | Claim | Bucket | Evidence |
|----|-------|--------|----------|
| D1 | k = 4 dimensions (Data stack, Return stack, Dictionary, I/O) | Definition | Architecture |
| D2 | Split LSB/MSB stack in zero page | Definition | `core.asm` layout |
| D3 | Subroutine threading via JSR/RTS | Definition | `compiler.asm` |
| G1 | DUP executes in 22 cycles | Guarantee | `PERFORMANCE.md` timing |
| G2 | SWAP executes in 26 cycles | Guarantee | `PERFORMANCE.md` timing |
| G3 | Tail-call elimination replaces JSR+RTS with JMP | Guarantee | `compiler.asm` |
| G4 | All core words follow stack effect contracts | Guarantee | Forth 2012 standard tests |
| A1 | 32-cell data stack sufficient for typical programs | Assumption | — |
| A2 | Linked-list dictionary search acceptable at C64 word counts | Assumption | — |
| U1 | Performance impact of very deep dictionary chains (1000+ words) | Unknown | — |

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

- Forth 2012 standard compatible
- Subroutine-threaded execution
- Zero-page optimized data stack
- Built-in editor and graphics support
- Exception handling (CATCH/THROW)

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

Core word timings (1 MHz 6502):

| Word | Cycles |
|------|--------|
| DUP | 22 |
| SWAP | 26 |
| + | 18 |

See [PERFORMANCE.md](PERFORMANCE.md) for analysis.

## Documentation

- `manual/` - Complete reference manual (build with `make manual.pdf`)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## License

Apache-2.0 © 2026 Jacob Coleman — See [LICENSE](LICENSE) for details.
