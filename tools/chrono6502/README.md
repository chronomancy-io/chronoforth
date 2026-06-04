# chrono6502

A headless, cycle-exact MOS 6502/6510 core (Rust, no dependencies) purpose-built
to measure and verify ChronoForth. It boots the **real** ChronoForth kernel
in-process — no VIC-II/SID/disk, just the CPU plus minimal KERNAL/IEC stubs that
serve Forth source from `forth/`/`test/` — so the whole Forth-2012 test suite
runs in ~0.4 s and any word's exact cycle cost is one command away.

## Build

```bash
cargo build --release
cargo test            # 11 unit tests of the cycle model
```

It needs the assembled program and an ACME symbol dump:

```bash
make durexforth.prg                                       # from repo root
acme -I asm --vicelabels labels.vice asm/durexforth.asm
```

## Commands

```bash
chrono6502 --prg durexforth.prg --labels labels.vice <cmd>
```

| Command | What it does |
|---------|--------------|
| `selftest` | Measure 22 primitives, assert cycle counts + stack effects (exit 1 on mismatch) |
| `ledger`   | Print cycles for the curated primitive set |
| `word NAME [in…]` | JSR one word by symbol, print cycles + resulting stack |
| `boot "<forth>"` | Boot the full system, run a Forth one-liner, print output |
| `gate` | Run the entire Forth-2012 suite; exit 0 ⇔ 0 errors (the correctness gate) |
| `defcyc "<defs>" NAME [in…]` | Compile a definition, JSR-measure it on the post-boot image |

## How it measures

`call_word` sets up the split LSB/MSB zero-page stack (`X = 256 − n`, TOS at
index X, zero-page-X wrap), pushes a sentinel return address, sets PC to the
word, and counts cycles until the matching `RTS`. `boot`/`gate` run the kernel
from its SYS entry; a write to `$D7FF` halts with an exit code (mirroring VICE's
debug cart). The base.fs build-time `0 $d7ff c!` line is stripped on the fly so
the boot reaches the interactive prompt.

## Validation

1. Exact agreement with hand-derived cycle counts on 22 straight-line primitives.
2. `cargo test`: page-cross penalties, branch timing, ADC/SBC flags, ZP-X wrap, JSR/RTS.
3. The full Forth-2012 suite passes in-emulator.
4. VICE cross-check via a CIA-timer benchmark — the emulator reproduces VICE's
   counts including placement-dependent branch page-cross.

See [../../PERFORMANCE.md](../../PERFORMANCE.md) §Methodology.
