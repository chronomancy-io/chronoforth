//! chrono6502 — a headless, cycle-exact 6502 harness for ChronoForth.
//!
//! Loads the assembled `durexforth.prg`, JSRs into a word by its symbol
//! address, counts exact cycles to the matching RTS, and reads back the split
//! LSB/MSB zero-page data stack. No VIC-II/SID/CIA/disk — just the CPU, which
//! is all that self-contained stack & math primitives need.

mod c64;
mod cpu;
mod disasm;
mod sym;

use cpu::{Cpu, Ram};
use std::collections::HashMap;
use sym::Symbols;

const LSB_BASE: u16 = 0x3b;
const MSB_BASE: u16 = 0x73;
const SENTINEL: u16 = 0xFFF0; // RTS lands here; never executed as code

struct Outcome {
    cycles: u64,
    out: Vec<u16>, // index 0 = deepest, last = TOS
}

/// Set up `inputs` (index 0 = deepest, last = TOS) on the split stack, JSR the
/// word at `addr`, run to RTS, and read back the resulting stack. The fresh prg
/// image is loaded at `load` (words may self-modify their own code).
fn call_word(image: &[u8], load: u16, addr: u16, inputs: &[u16]) -> Result<Outcome, String> {
    let mut base = vec![0u8; 65536];
    base[load as usize..load as usize + image.len()].copy_from_slice(image);
    call_word_mem(&base, addr, inputs)
}

/// Like `call_word`, but runs against a full 64 KiB memory snapshot (e.g. a
/// post-boot image whose dictionary the measured word refers into).
fn call_word_mem(snapshot: &[u8], addr: u16, inputs: &[u16]) -> Result<Outcome, String> {
    let mut ram = Ram::default();
    ram.mem.copy_from_slice(&snapshot[..65536]);
    let mut cpu = Cpu::new(ram);

    let n = inputs.len() as u8;
    let x0 = 0u8.wrapping_sub(n); // X = 256 - n
    cpu.x = x0;
    for (i, &val) in inputs.iter().enumerate() {
        // inputs[last] = TOS at index x0; inputs[0] = deepest at x0+(n-1)
        let depth_from_top = (inputs.len() - 1 - i) as u8;
        let idx = x0.wrapping_add(depth_from_top);
        let lo_addr = ((LSB_BASE.wrapping_add(idx as u16)) & 0x00FF) as usize;
        let hi_addr = ((MSB_BASE.wrapping_add(idx as u16)) & 0x00FF) as usize;
        cpu.bus.mem[lo_addr] = (val & 0xFF) as u8;
        cpu.bus.mem[hi_addr] = (val >> 8) as u8;
    }

    // push sentinel return address so the word's final RTS lands at SENTINEL
    cpu.sp = 0xFF;
    cpu.pushw(SENTINEL.wrapping_sub(1));
    cpu.pc = addr;

    let cycles = cpu.run_until(SENTINEL, 5_000_000).map_err(|e| e.0)?;

    // read back: depth = 256 - X'
    let xf = cpu.x;
    let depth = (0u8.wrapping_sub(xf)) as usize;
    let mut out = Vec::with_capacity(depth);
    for j in 0..depth {
        let depth_from_top = (depth - 1 - j) as u8;
        let idx = xf.wrapping_add(depth_from_top);
        let lo = cpu.bus.mem[((LSB_BASE.wrapping_add(idx as u16)) & 0xFF) as usize] as u16;
        let hi = cpu.bus.mem[((MSB_BASE.wrapping_add(idx as u16)) & 0xFF) as usize] as u16;
        out.push(lo | (hi << 8));
    }
    Ok(Outcome { cycles, out })
}

/// A word to measure: forth name, asm label, inputs (deepest..TOS), optional
/// expected output and cycle count for the self-test.
struct Spec {
    forth: &'static str,
    label: &'static str,
    inputs: &'static [u16],
    expect_out: Option<&'static [u16]>,
    expect_cycles: Option<u64>,
}

fn ledger_specs() -> Vec<Spec> {
    vec![
        Spec { forth: "drop", label: "DROP",     inputs: &[0x1111, 0x2222], expect_out: Some(&[0x1111]),         expect_cycles: Some(14) },
        Spec { forth: "dup",  label: "DUP",      inputs: &[0x2222],         expect_out: Some(&[0x2222, 0x2222]), expect_cycles: Some(24) },
        Spec { forth: "?dup", label: "QDUP",     inputs: &[0x2222],         expect_out: Some(&[0x2222, 0x2222]), expect_cycles: None },
        Spec { forth: "swap", label: "SWAP",     inputs: &[0xAAAA, 0xBBBB], expect_out: Some(&[0xBBBB, 0xAAAA]), expect_cycles: Some(38) },
        Spec { forth: "over", label: "OVER",     inputs: &[0xAAAA, 0xBBBB], expect_out: Some(&[0xAAAA, 0xBBBB, 0xAAAA]), expect_cycles: Some(24) },
        Spec { forth: "nip",  label: "NIP",      inputs: &[0xAAAA, 0xBBBB], expect_out: Some(&[0xBBBB]),         expect_cycles: None },
        Spec { forth: "tuck", label: "TUCK",     inputs: &[0xAAAA, 0xBBBB], expect_out: Some(&[0xBBBB, 0xAAAA, 0xBBBB]), expect_cycles: None },
        Spec { forth: "rot",  label: "ROT",      inputs: &[1, 2, 3],        expect_out: Some(&[2, 3, 1]),        expect_cycles: None },
        Spec { forth: "2dup", label: "TWODUP",   inputs: &[0xAAAA, 0xBBBB], expect_out: Some(&[0xAAAA, 0xBBBB, 0xAAAA, 0xBBBB]), expect_cycles: None },
        Spec { forth: "1+",   label: "ONEPLUS",  inputs: &[0x0001],         expect_out: Some(&[0x0002]),         expect_cycles: Some(15) },
        Spec { forth: "1-",   label: "ONEMINUS", inputs: &[0x0002],         expect_out: Some(&[0x0001]),         expect_cycles: None },
        Spec { forth: "+",    label: "PLUS",     inputs: &[0x0003, 0x0004], expect_out: Some(&[0x0007]),         expect_cycles: Some(34) },
        Spec { forth: "-",    label: "MINUS",    inputs: &[0x0009, 0x0004], expect_out: Some(&[0x0005]),         expect_cycles: Some(34) },
        Spec { forth: "=",    label: "EQUAL",    inputs: &[0x0005, 0x0005], expect_out: Some(&[0xFFFF]),         expect_cycles: None },
        Spec { forth: "0=",   label: "ZEQU",     inputs: &[0x0000],         expect_out: Some(&[0xFFFF]),         expect_cycles: None },
        Spec { forth: "<",    label: "LESS_THAN",inputs: &[0x0003, 0x0005], expect_out: Some(&[0xFFFF]),         expect_cycles: None },
        Spec { forth: ">",    label: "GREATER_THAN", inputs: &[0x0005, 0x0003], expect_out: Some(&[0xFFFF]),     expect_cycles: None },
        Spec { forth: "u<",   label: "U_LESS",   inputs: &[0x0003, 0x0005], expect_out: Some(&[0xFFFF]),         expect_cycles: None },
        Spec { forth: "invert", label: "INVERT", inputs: &[0x0F0F],         expect_out: Some(&[0xF0F0]),         expect_cycles: None },
        Spec { forth: "negate", label: "NEGATE", inputs: &[0x0001],         expect_out: Some(&[0xFFFF]),         expect_cycles: None },
        Spec { forth: "max",  label: "MAX",      inputs: &[0x0003, 0x0009], expect_out: Some(&[0x0009]),         expect_cycles: None },
        Spec { forth: "min",  label: "MIN",      inputs: &[0x0003, 0x0009], expect_out: Some(&[0x0003]),         expect_cycles: None },
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut prg = "durexforth.prg".to_string();
    let mut labels = "labels.vice".to_string();
    let mut repo = "../..".to_string();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--prg" => { prg = args[i + 1].clone(); i += 2; }
            "--labels" => { labels = args[i + 1].clone(); i += 2; }
            "--repo" => { repo = args[i + 1].clone(); i += 2; }
            _ => { rest.push(args[i].clone()); i += 1; }
        }
    }
    let cmd = rest.first().map(|s| s.as_str()).unwrap_or("ledger").to_string();

    let bytes = std::fs::read(&prg).unwrap_or_else(|e| { eprintln!("read {}: {}", prg, e); std::process::exit(2); });
    let load = bytes[0] as u16 | ((bytes[1] as u16) << 8);
    let image = bytes[2..].to_vec();
    let load_syms = || Symbols::load(&labels).unwrap_or_else(|e| { eprintln!("read {}: {}", labels, e); std::process::exit(2); });

    match cmd.as_str() {
        "selftest" => {
            let syms = load_syms();
            let mut fail = 0;
            println!("{:<8} {:>6} {:>8} {}", "word", "addr", "cycles", "result");
            for s in ledger_specs() {
                let addr = match syms.addr(s.label) { Some(a) => a, None => { println!("{:<8}   (no label {})", s.forth, s.label); continue; } };
                match call_word(&image, load, addr, s.inputs) {
                    Ok(o) => {
                        let mut status = String::new();
                        if let Some(exp) = s.expect_out {
                            if o.out != exp { status += &format!(" OUT-FAIL exp={:04X?} got={:04X?}", exp, o.out); fail += 1; }
                        }
                        if let Some(ec) = s.expect_cycles {
                            if o.cycles != ec { status += &format!(" CYC-FAIL exp={} got={}", ec, o.cycles); fail += 1; }
                        }
                        if status.is_empty() { status = "ok".into(); }
                        println!("{:<8} ${:04X} {:>8} {}", s.forth, addr, o.cycles, status);
                    }
                    Err(e) => { println!("{:<8} ${:04X}   ERROR {}", s.forth, addr, e); fail += 1; }
                }
            }
            if fail > 0 { eprintln!("\nSELFTEST FAILED: {} mismatch(es)", fail); std::process::exit(1); }
            println!("\nselftest OK");
        }
        "ledger" => {
            let syms = load_syms();
            println!("{:<8} {:>6} {:>8}", "word", "addr", "cycles");
            for s in ledger_specs() {
                let addr = match syms.addr(s.label) { Some(a) => a, None => continue };
                match call_word(&image, load, addr, s.inputs) {
                    Ok(o) => println!("{:<8} ${:04X} {:>8}", s.forth, addr, o.cycles),
                    Err(e) => println!("{:<8} ${:04X}   ERROR {}", s.forth, addr, e),
                }
            }
        }
        "word" => {
            let syms = load_syms();
            let label = rest.get(1).cloned().unwrap_or_default();
            let inputs: Vec<u16> = rest[2..].iter().map(|t| parse_u16(t)).collect();
            let addr = syms.addr(&label).unwrap_or_else(|| { eprintln!("no symbol {}", label); std::process::exit(2); });
            match call_word(&image, load, addr, &inputs) {
                Ok(o) => println!("{} ${:04X}: {} cycles, out={:04X?}", label, addr, o.cycles, o.out),
                Err(e) => { eprintln!("ERROR: {}", e); std::process::exit(1); }
            }
        }
        "boot" => {
            // run a Forth one-liner through the fully booted system
            let line = rest[1..].join(" ");
            let kbd = format!("{}\n", line);
            let files = load_src(&repo);
            let r = c64::boot_and_run(&image, load, files, &kbd, 500_000_000);
            print!("{}", r.output);
            println!("\n--- exit_code={:?} cycles={} steps={} final_pc=${:04X} ---", r.exit_code, r.cycles, r.steps, r.final_pc);
            if std::env::var("CHRONO_DUMP").is_ok() {
                let base = (r.final_pc.saturating_sub(0x30)) & 0xFFF0;
                for row in 0..6u16 {
                    let a = base + row * 16;
                    eprint!("{:04X}:", a);
                    for c in 0..16u16 { eprint!(" {:02X}", r.mem[(a + c) as usize]); }
                    eprintln!();
                }
            }
        }
        "defcyc" => {
            // measure a compiled definition's cycles:
            //   defcyc "<defs ending in the word>" <wordname> <inputs...>
            // boots, defines, stashes the word's xt at $C000, then JSR-measures
            // it on the post-boot snapshot.
            let defsrc = rest.get(1).cloned().unwrap_or_default();
            let name = rest.get(2).cloned().unwrap_or_default();
            let inputs: Vec<u16> = rest.get(3..).unwrap_or(&[]).iter().map(|t| parse_u16(t)).collect();
            let kbd = format!("decimal {} ' {} $c000 ! 0 $d7ff c!\n", defsrc, name);
            let files = load_src(&repo);
            let r = c64::boot_and_run(&image, load, files, &kbd, 500_000_000);
            if r.exit_code != Some(0) {
                eprintln!("boot/define failed (exit={:?}); output tail:\n{}", r.exit_code,
                    r.output.lines().rev().take(5).collect::<Vec<_>>().join("\n"));
                std::process::exit(1);
            }
            let xt = r.mem[0xC000] as u16 | ((r.mem[0xC001] as u16) << 8);
            if xt == 0 { eprintln!("xt not captured at $C000"); std::process::exit(1); }
            match call_word_mem(&r.mem, xt, &inputs) {
                Ok(o) => println!("{:<10} xt=${:04X}  {:>6} cycles  out={:04X?}", name, xt, o.cycles, o.out),
                Err(e) => { eprintln!("ERROR: {}", e); std::process::exit(1); }
            }
        }
        "dis" => {
            // dis "<forth defs>" <wordname> [count]
            // boot, define, then walk the dictionary and disassemble <wordname>
            let defsrc = rest.get(1).cloned().unwrap_or_default();
            let name = rest.get(2).cloned().unwrap_or_default();
            let n: usize = rest.get(3).and_then(|s| s.parse().ok()).unwrap_or(40);
            let kbd = format!("decimal {} 0 $d7ff c!\n", defsrc);
            let files = load_src(&repo);
            let r = c64::boot_and_run(&image, load, files, &kbd, 500_000_000);
            let words = disasm::walk(&r.mem);
            match disasm::find(&words, &name) {
                Some(w) => {
                    println!("{}  nt=${:04X} xt=${:04X} flags=${:02X}", w.name, w.nt, w.xt, w.flags);
                    for line in disasm::disasm(&r.mem, w.xt, n, &words) { println!("{}", line); }
                }
                None => {
                    eprintln!("word {:?} not found (exit={:?}); known words near loop:", name, r.exit_code);
                    for w in words.iter().filter(|w| w.name.contains("loop") || w.name.contains("do")) {
                        eprintln!("  {} xt=${:04X}", w.name, w.xt);
                    }
                }
            }
        }
        "dict" => {
            let files = load_src(&repo);
            let r = c64::boot_and_run(&image, load, files, "decimal 0 $d7ff c!\n", 500_000_000);
            let words = disasm::walk(&r.mem);
            println!("{} words (exit={:?})", words.len(), r.exit_code);
            for w in &words { println!("  {:<14} nt=${:04X} xt=${:04X} flags=${:02X}", w.name, w.nt, w.xt, w.flags); }
        }
        "disaddr" => {
            // disaddr "<forth defs>" <hexaddr> [count]
            // boot, optionally define, then disassemble at a raw address.
            let defsrc = rest.get(1).cloned().unwrap_or_default();
            let addr = parse_u16(&rest.get(2).cloned().unwrap_or_default());
            let n: usize = rest.get(3).and_then(|s| s.parse().ok()).unwrap_or(40);
            let kbd = format!("decimal {} 0 $d7ff c!\n", defsrc);
            let files = load_src(&repo);
            let r = c64::boot_and_run(&image, load, files, &kbd, 500_000_000);
            let words = disasm::walk(&r.mem);
            println!("disaddr ${:04X} (exit={:?})", addr, r.exit_code);
            for line in disasm::disasm(&r.mem, addr, n, &words) { println!("{}", line); }
        }
        "gate" => {
            // run the standard forth-2012 test suite; exit code = #errors
            let mut files = load_src(&repo);
            let check = "\
parse-name compat included\n\
parse-name tester included\n\
: error cr type source type empty-stack #errors @ 1+ #errors ! ;\n\
parse-name testcore included\n\
parse-name testcoreplus included\n\
parse-name testcoreext included\n\
parse-name testexception included\n\
decimal #errors @ $d7ff c!\n";
            files.insert("check".to_string(), check.as_bytes().to_vec());
            let r = c64::boot_and_run(&image, load, files, "include check\n", 2_000_000_000);
            print!("{}", r.output);
            match r.exit_code {
                Some(0) => { eprintln!("\n[GATE] PASS (0 errors, {} cycles)", r.cycles); }
                Some(n) => { eprintln!("\n[GATE] FAIL: {} error(s)", n); std::process::exit(1); }
                None => { eprintln!("\n[GATE] INCONCLUSIVE (no exit; {} steps)", r.steps); std::process::exit(2); }
            }
        }
        other => { eprintln!("unknown command: {}", other); std::process::exit(2); }
    }
}

/// Load all `forth/*.fs` and `test/*.fs` from the repo into a name->content map.
fn load_src(repo: &str) -> HashMap<String, Vec<u8>> {
    let mut m = HashMap::new();
    for sub in ["forth", "test"] {
        let dir = format!("{}/{}", repo, sub);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) == Some("fs") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(data) = std::fs::read(&path) {
                            let stem = stem.to_lowercase();
                            // base.fs is durexforth's build script; it ends with
                            // `0 $d7ff c!` to exit the build VICE. Strip that line
                            // so the harness boot falls through to the QUIT prompt.
                            let data = if stem == "base" {
                                String::from_utf8_lossy(&data)
                                    .lines()
                                    .filter(|l| !l.contains("$d7ff"))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                                    .into_bytes()
                            } else {
                                data
                            };
                            m.insert(stem, data);
                        }
                    }
                }
            }
        }
    }
    m
}

fn parse_u16(t: &str) -> u16 {
    let t = t.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("$")) {
        u16::from_str_radix(h, 16).unwrap_or(0)
    } else {
        t.parse::<i32>().map(|v| v as u16).unwrap_or(0)
    }
}
