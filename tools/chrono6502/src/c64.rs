//! Minimal C64 environment: RAM + just enough KERNAL/IEC stubs to BOOT the real
//! ChronoForth kernel and run Forth source headlessly. Files are served from a
//! host map (petcat `-text` transform applied on the fly); console input comes
//! from an injected buffer; CHROUT is captured; a write to $D7FF halts the
//! machine with an exit code (mirrors VICE's debug cart, so the test gate is
//! identical to the VICE one).

use crate::cpu::{Bus, Cpu, C, N, Z};
use std::collections::HashMap;

// KERNAL / editor entry points we intercept.
const CHROUT: u16 = 0xFFD2;
const SETNAM: u16 = 0xFFBD;
const SETLFS: u16 = 0xFFBA;
const OPEN: u16 = 0xFFC0;
const CLOSE: u16 = 0xFFC3;
const CHKIN: u16 = 0xFFC6;
const CHKOUT: u16 = 0xFFC9;
const CLRCHN: u16 = 0xFFCC;
const CHRIN: u16 = 0xFFCF;
const READST: u16 = 0xFFB7;
const LOAD: u16 = 0xFFD5;
const SAVE: u16 = 0xFFD8;
const GETIN: u16 = 0xFFE4;
const STOP: u16 = 0xFFE1;
const ED_INPUT: u16 = 0xE112; // editor "input a character"
const ED_GETKEY: u16 = 0xE5B4; // get char from keyboard buffer

struct Stream {
    data: Vec<u8>,
    pos: usize,
}

pub struct C64Bus {
    pub mem: Box<[u8; 65536]>,
    pub files: HashMap<String, Vec<u8>>, // lowercase name -> RAW ascii content
    pub out: Vec<u8>,                     // captured CHROUT bytes (petscii)
    pub halt: Option<u8>,
    kbd: Vec<u8>, // injected console input (already petscii, with CRs)
    kbd_pos: usize,
    streams: HashMap<u8, Stream>, // logical file# -> open input stream
    cur_in: u8,                   // current input channel (0 = keyboard)
    status: u8,                   // KERNAL ST
    setnam: (u16, u8),            // (ptr, len)
    setlfs_la: u8,
    pub last_pc: u16,             // PC of the instruction currently executing (for write-watch)
    pub watch: bool,              // log writes into kernel code region
}

/// petcat `-text`: ASCII -> PETSCII (case swap) and LF -> CR.
fn to_petscii(src: &[u8]) -> Vec<u8> {
    src.iter()
        .map(|&b| match b {
            0x0A => 0x0D,
            0x61..=0x7A => b - 0x20, // a-z -> $41-$5A
            0x41..=0x5A => b + 0x80, // A-Z -> $C1-$DA
            _ => b,
        })
        .collect()
}

/// PETSCII filename/letter -> lowercase ASCII (for host file lookup & output).
fn petscii_to_ascii_lower(b: u8) -> u8 {
    match b {
        0x41..=0x5A => b + 0x20,        // uppercase region -> a-z
        0xC1..=0xDA => b - 0xC1 + 0x61, // shifted letters -> a-z
        _ => b,
    }
}

impl C64Bus {
    pub fn new(files: HashMap<String, Vec<u8>>, keyboard: &str) -> Self {
        C64Bus {
            mem: Box::new([0u8; 65536]),
            files,
            out: Vec::new(),
            halt: None,
            kbd: to_petscii(keyboard.as_bytes()),
            kbd_pos: 0,
            streams: HashMap::new(),
            cur_in: 0,
            status: 0,
            setnam: (0, 0),
            setlfs_la: 0,
            last_pc: 0,
            watch: std::env::var("CHRONO_WATCH").is_ok(),
        }
    }

    /// Served file bytes = 2-byte dummy load address + transformed content.
    fn serve(&self, name: &str) -> Option<Vec<u8>> {
        self.files.get(name).map(|raw| {
            let mut v = vec![0x01, 0x08];
            v.extend(to_petscii(raw));
            v
        })
    }

    pub fn output_string(&self) -> String {
        self.out
            .iter()
            .map(|&b| match b {
                0x0D => '\n',
                _ => petscii_to_ascii_lower(b) as char,
            })
            .collect()
    }
}

impl Bus for C64Bus {
    fn read(&mut self, addr: u16) -> u8 {
        self.mem[addr as usize]
    }
    fn write(&mut self, addr: u16, val: u8) {
        if addr == 0xD7FF {
            self.halt = Some(val);
            return;
        }
        if self.watch && (0x0801..0x1748).contains(&addr) {
            eprintln!("[WATCH] write ${:02X} -> ${:04X}  (pc=${:04X})", val, addr, self.last_pc);
        }
        self.mem[addr as usize] = val;
    }
}

pub struct RunResult {
    pub exit_code: Option<u8>,
    pub output: String,
    pub cycles: u64,
    pub steps: u64,
    pub final_pc: u16,
    pub mem: Vec<u8>,
}

/// Is `pc` a routine we intercept?
fn is_trap(pc: u16) -> bool {
    matches!(
        pc,
        CHROUT | SETNAM | SETLFS | OPEN | CLOSE | CHKIN | CHKOUT | CLRCHN | CHRIN | READST
            | LOAD | SAVE | GETIN | STOP | ED_INPUT | ED_GETKEY
    ) || (0xFF90..=0xFFB6).contains(&pc) // misc IEC routines (LISTEN/TALK/etc.) -> no-op
}

/// Execute the intercepted routine using the CPU registers + bus state, then RTS.
fn kernal(cpu: &mut Cpu<C64Bus>, pc: u16) {
    cpu.cycles += 20; // nominal; KERNAL time is irrelevant to the word ledger
    if std::env::var("CHRONO_TRACE").is_ok() {
        let name = match pc {
            CHROUT => "CHROUT", SETNAM => "SETNAM", SETLFS => "SETLFS", OPEN => "OPEN",
            CLOSE => "CLOSE", CHKIN => "CHKIN", CHKOUT => "CHKOUT", CLRCHN => "CLRCHN",
            CHRIN => "CHRIN", READST => "READST", LOAD => "LOAD", SAVE => "SAVE",
            GETIN => "GETIN", STOP => "STOP", ED_INPUT => "ED_INPUT", ED_GETKEY => "ED_GETKEY",
            0xFFA5 => "IECIN", 0xFFAE => "UNLSN", _ => "misc",
        };
        if name != "CHROUT" {
            eprintln!("  k {:<7} A={:02X} X={:02X} Y={:02X} cur_in={} st={:02X}",
                name, cpu.a, cpu.x, cpu.y, cpu.bus.cur_in, cpu.bus.status);
        }
    }
    match pc {
        CHROUT => {
            let a = cpu.a;
            cpu.bus.out.push(a);
        }
        SETNAM => {
            let len = cpu.a;
            let ptr = (cpu.x as u16) | ((cpu.y as u16) << 8);
            cpu.bus.setnam = (ptr, len);
        }
        SETLFS => {
            cpu.bus.setlfs_la = cpu.a;
        }
        OPEN => {
            let (ptr, len) = cpu.bus.setnam;
            let mut name = String::new();
            for i in 0..len as u16 {
                let b = cpu.bus.mem[(ptr.wrapping_add(i)) as usize];
                name.push(petscii_to_ascii_lower(b) as char);
            }
            let name = name.to_lowercase();
            let found = cpu.bus.serve(&name);
            if std::env::var("CHRONO_DEBUG").is_ok() {
                let rawlen = cpu.bus.files.get(&name).map(|c| c.len()).unwrap_or(0);
                eprintln!("[OPEN] name={:?} found={} rawlen={} la={}", name, found.is_some(), rawlen, cpu.bus.setlfs_la);
            }
            match found {
                Some(bytes) => {
                    cpu.bus.streams.insert(cpu.bus.setlfs_la, Stream { data: bytes, pos: 0 });
                    cpu.p &= !C; // success
                }
                None => {
                    cpu.a = 4; // file not found
                    cpu.p |= C;
                }
            }
        }
        CHKIN => {
            let la = cpu.x;
            if cpu.bus.streams.contains_key(&la) {
                cpu.bus.cur_in = la;
                cpu.bus.status = 0;
            }
            cpu.p &= !C;
        }
        CHKOUT => {
            cpu.p &= !C;
        }
        CLRCHN => {
            cpu.bus.cur_in = 0;
        }
        CHRIN => {
            let la = cpu.bus.cur_in;
            if let Some(s) = cpu.bus.streams.get_mut(&la) {
                if s.pos < s.data.len() {
                    let b = s.data[s.pos];
                    s.pos += 1;
                    cpu.bus.status = if s.pos >= s.data.len() { 0x40 } else { 0 };
                    cpu.a = b;
                } else {
                    cpu.bus.status = 0x40;
                    cpu.a = 0;
                }
            } else {
                cpu.a = 0;
            }
        }
        READST => {
            cpu.a = cpu.bus.status;
        }
        CLOSE => {
            let la = cpu.a;
            cpu.bus.streams.remove(&la);
            cpu.p &= !C;
        }
        ED_INPUT => {
            // console "input a character": serve injected keyboard bytes
            if cpu.bus.kbd_pos < cpu.bus.kbd.len() {
                cpu.a = cpu.bus.kbd[cpu.bus.kbd_pos];
                cpu.bus.kbd_pos += 1;
            } else {
                // input exhausted without an explicit exit -> end the session
                // (a driver that completes halts itself via $D7FF first).
                cpu.bus.halt = Some(254);
                cpu.a = 0x0D;
            }
        }
        ED_GETKEY => {
            cpu.a = if cpu.bus.kbd_pos < cpu.bus.kbd.len() {
                let b = cpu.bus.kbd[cpu.bus.kbd_pos];
                cpu.bus.kbd_pos += 1;
                b
            } else {
                0x0D
            };
        }
        GETIN => {
            cpu.a = 0; // no key
        }
        STOP => {
            cpu.p |= C; // not pressed (Z clear path)
            cpu.p &= !crate::cpu::Z;
        }
        LOAD | SAVE => {
            cpu.p &= !C; // pretend success, do nothing
        }
        0xFFAE => {
            // UNLSN: mark device present, no error (clears ST so _errorchread proceeds)
            cpu.bus.mem[0x90] = 0;
        }
        0xFFA5 => {
            // IECIN (read byte from IEC): return CR + flag EOF so the
            // read-error-channel loop in _errorchread terminates at once.
            cpu.bus.mem[0x90] = 0x40;
            cpu.a = 0x0D;
        }
        _ => { /* misc IEC routines (LISTEN/TALK/...): no-op */ }
    }
    // Value-returning KERNAL routines return with Z/N reflecting A (callers
    // branch on it, e.g. INCLUDED's `jsr READST / beq`).
    if matches!(pc, READST | CHRIN | GETIN | ED_INPUT | ED_GETKEY | 0xFFA5) {
        let a = cpu.a;
        cpu.p = (cpu.p & !(Z | N)) | (if a == 0 { Z } else { 0 }) | (a & N);
    }
    // simulate RTS
    let ret = cpu.popw().wrapping_add(1);
    cpu.pc = ret;
}

/// Boot ChronoForth from `image` (loaded at `load`) and feed `keyboard` to the
/// console. Returns when $D7FF is written (exit_code) or step budget is hit.
pub fn boot_and_run(
    image: &[u8],
    load: u16,
    files: HashMap<String, Vec<u8>>,
    keyboard: &str,
    max_steps: u64,
) -> RunResult {
    let mut bus = C64Bus::new(files, keyboard);
    for (i, b) in image.iter().enumerate() {
        bus.mem[load as usize + i] = *b;
    }
    // $01 processor port default; KERNAL present
    bus.mem[0x0001] = 0x37;
    let mut cpu = Cpu::new(bus);
    cpu.pc = load + 0x0C; // SYS 2061 entry, past the 12-byte BASIC stub
    cpu.sp = 0xFD;

    let dbg = std::env::var("CHRONO_DEBUG").is_ok();
    let trace_crash = std::env::var("CHRONO_CRASH").is_ok();
    let mut recent = [0u16; 64];
    let mut ri = 0usize;
    let mut steps = 0u64;
    loop {
        if trace_crash {
            recent[ri % 64] = cpu.pc;
            ri += 1;
            if cpu.pc < 0x0700 && !is_trap(cpu.pc) {
                eprintln!("[CRASH] pc=${:04X} sp=${:02X} step={}", cpu.pc, cpu.sp, steps);
                eprint!("  recent PCs:");
                for k in 0..64 { eprint!(" {:04X}", recent[(ri + k) % 64]); }
                eprintln!();
                eprint!("  rstack:");
                for s in (cpu.sp as u16 + 1)..=0xFF { eprint!(" {:02X}", cpu.bus.mem[0x100 + s as usize]); }
                eprintln!();
                return RunResult { exit_code: Some(253), output: cpu.bus.output_string(), cycles: cpu.cycles, steps, final_pc: cpu.pc, mem: cpu.bus.mem.to_vec() };
            }
        }
        if let Some(code) = cpu.bus.halt {
            return RunResult { exit_code: Some(code), output: cpu.bus.output_string(), cycles: cpu.cycles, steps, final_pc: cpu.pc, mem: cpu.bus.mem.to_vec() };
        }
        if dbg && cpu.pc == 0x0DBB {
            // throw_a: A = error code (signed)
            let tib_size = cpu.bus.mem[0x02ad]; // best-effort; print raw TIB
            let _ = tib_size;
            let mut line = String::new();
            for k in 0..40u16 {
                let b = cpu.bus.mem[(0x0200 + k) as usize];
                if b == 0 { break; }
                line.push(petscii_to_ascii_lower(b) as char);
            }
            eprintln!("[THROW] code={} step={} TIB~={:?}", cpu.a as i8, steps, line);
        }
        cpu.bus.last_pc = cpu.pc;
        if is_trap(cpu.pc) {
            let pc = cpu.pc;
            kernal(&mut cpu, pc);
        } else if let Err(e) = cpu.step() {
            if trace_crash {
                eprint!("[ERR] {} recent PCs:", e.0);
                for k in 0..64 { eprint!(" {:04X}", recent[(ri + k) % 64]); }
                eprint!("\n  rstack:");
                for s in (cpu.sp as u16 + 1)..=0xFF { eprint!(" {:02X}", cpu.bus.mem[0x100 + s as usize]); }
                eprintln!();
            }
            let mut out = cpu.bus.output_string();
            out.push_str(&format!("\n[CPU ERROR] {}", e.0));
            return RunResult { exit_code: None, output: out, cycles: cpu.cycles, steps, final_pc: cpu.pc, mem: cpu.bus.mem.to_vec() };
        }
        steps += 1;
        if steps > max_steps {
            return RunResult { exit_code: None, output: cpu.bus.output_string(), cycles: cpu.cycles, steps, final_pc: cpu.pc, mem: cpu.bus.mem.to_vec() };
        }
    }
}
