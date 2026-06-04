//! Minimal 6502 disassembler + ChronoForth dictionary walker, for decoding the
//! dynamically-compiled words (e.g. `(loop)`, `(do)`, user definitions) that
//! have no ACME symbol.

#[derive(Clone, Copy, PartialEq)]
pub enum Mode { Imp, Acc, Imm, Zp, ZpX, ZpY, Abs, AbsX, AbsY, Ind, IndX, IndY, Rel }

impl Mode {
    pub fn len(self) -> usize {
        match self {
            Mode::Imp | Mode::Acc => 1,
            Mode::Imm | Mode::Zp | Mode::ZpX | Mode::ZpY | Mode::IndX | Mode::IndY | Mode::Rel => 2,
            Mode::Abs | Mode::AbsX | Mode::AbsY | Mode::Ind => 3,
        }
    }
}

/// (mnemonic, mode) for an opcode. Unknown/illegal -> ("???", Imp).
pub fn decode(op: u8) -> (&'static str, Mode) {
    use Mode::*;
    match op {
        0x00 => ("brk", Imp), 0x40 => ("rti", Imp), 0x60 => ("rts", Imp), 0xEA => ("nop", Imp),
        0x20 => ("jsr", Abs), 0x4C => ("jmp", Abs), 0x6C => ("jmp", Ind),
        0x10 => ("bpl", Rel), 0x30 => ("bmi", Rel), 0x50 => ("bvc", Rel), 0x70 => ("bvs", Rel),
        0x90 => ("bcc", Rel), 0xB0 => ("bcs", Rel), 0xD0 => ("bne", Rel), 0xF0 => ("beq", Rel),
        0x18 => ("clc", Imp), 0x38 => ("sec", Imp), 0x58 => ("cli", Imp), 0x78 => ("sei", Imp),
        0xB8 => ("clv", Imp), 0xD8 => ("cld", Imp), 0xF8 => ("sed", Imp),
        0xAA => ("tax", Imp), 0x8A => ("txa", Imp), 0xA8 => ("tay", Imp), 0x98 => ("tya", Imp),
        0xBA => ("tsx", Imp), 0x9A => ("txs", Imp),
        0x48 => ("pha", Imp), 0x68 => ("pla", Imp), 0x08 => ("php", Imp), 0x28 => ("plp", Imp),
        0xE8 => ("inx", Imp), 0xC8 => ("iny", Imp), 0xCA => ("dex", Imp), 0x88 => ("dey", Imp),
        // LDA
        0xA9 => ("lda", Imm), 0xA5 => ("lda", Zp), 0xB5 => ("lda", ZpX), 0xAD => ("lda", Abs),
        0xBD => ("lda", AbsX), 0xB9 => ("lda", AbsY), 0xA1 => ("lda", IndX), 0xB1 => ("lda", IndY),
        // LDX
        0xA2 => ("ldx", Imm), 0xA6 => ("ldx", Zp), 0xB6 => ("ldx", ZpY), 0xAE => ("ldx", Abs), 0xBE => ("ldx", AbsY),
        // LDY
        0xA0 => ("ldy", Imm), 0xA4 => ("ldy", Zp), 0xB4 => ("ldy", ZpX), 0xAC => ("ldy", Abs), 0xBC => ("ldy", AbsX),
        // STA
        0x85 => ("sta", Zp), 0x95 => ("sta", ZpX), 0x8D => ("sta", Abs), 0x9D => ("sta", AbsX),
        0x99 => ("sta", AbsY), 0x81 => ("sta", IndX), 0x91 => ("sta", IndY),
        // STX/STY
        0x86 => ("stx", Zp), 0x96 => ("stx", ZpY), 0x8E => ("stx", Abs),
        0x84 => ("sty", Zp), 0x94 => ("sty", ZpX), 0x8C => ("sty", Abs),
        // AND/ORA/EOR
        0x29 => ("and", Imm), 0x25 => ("and", Zp), 0x35 => ("and", ZpX), 0x2D => ("and", Abs),
        0x3D => ("and", AbsX), 0x39 => ("and", AbsY), 0x21 => ("and", IndX), 0x31 => ("and", IndY),
        0x09 => ("ora", Imm), 0x05 => ("ora", Zp), 0x15 => ("ora", ZpX), 0x0D => ("ora", Abs),
        0x1D => ("ora", AbsX), 0x19 => ("ora", AbsY), 0x01 => ("ora", IndX), 0x11 => ("ora", IndY),
        0x49 => ("eor", Imm), 0x45 => ("eor", Zp), 0x55 => ("eor", ZpX), 0x4D => ("eor", Abs),
        0x5D => ("eor", AbsX), 0x59 => ("eor", AbsY), 0x41 => ("eor", IndX), 0x51 => ("eor", IndY),
        // ADC/SBC
        0x69 => ("adc", Imm), 0x65 => ("adc", Zp), 0x75 => ("adc", ZpX), 0x6D => ("adc", Abs),
        0x7D => ("adc", AbsX), 0x79 => ("adc", AbsY), 0x61 => ("adc", IndX), 0x71 => ("adc", IndY),
        0xE9 => ("sbc", Imm), 0xE5 => ("sbc", Zp), 0xF5 => ("sbc", ZpX), 0xED => ("sbc", Abs),
        0xFD => ("sbc", AbsX), 0xF9 => ("sbc", AbsY), 0xE1 => ("sbc", IndX), 0xF1 => ("sbc", IndY),
        // CMP/CPX/CPY
        0xC9 => ("cmp", Imm), 0xC5 => ("cmp", Zp), 0xD5 => ("cmp", ZpX), 0xCD => ("cmp", Abs),
        0xDD => ("cmp", AbsX), 0xD9 => ("cmp", AbsY), 0xC1 => ("cmp", IndX), 0xD1 => ("cmp", IndY),
        0xE0 => ("cpx", Imm), 0xE4 => ("cpx", Zp), 0xEC => ("cpx", Abs),
        0xC0 => ("cpy", Imm), 0xC4 => ("cpy", Zp), 0xCC => ("cpy", Abs),
        // BIT
        0x24 => ("bit", Zp), 0x2C => ("bit", Abs),
        // INC/DEC
        0xE6 => ("inc", Zp), 0xF6 => ("inc", ZpX), 0xEE => ("inc", Abs), 0xFE => ("inc", AbsX),
        0xC6 => ("dec", Zp), 0xD6 => ("dec", ZpX), 0xCE => ("dec", Abs), 0xDE => ("dec", AbsX),
        // shifts
        0x0A => ("asl", Acc), 0x06 => ("asl", Zp), 0x16 => ("asl", ZpX), 0x0E => ("asl", Abs), 0x1E => ("asl", AbsX),
        0x4A => ("lsr", Acc), 0x46 => ("lsr", Zp), 0x56 => ("lsr", ZpX), 0x4E => ("lsr", Abs), 0x5E => ("lsr", AbsX),
        0x2A => ("rol", Acc), 0x26 => ("rol", Zp), 0x36 => ("rol", ZpX), 0x2E => ("rol", Abs), 0x3E => ("rol", AbsX),
        0x6A => ("ror", Acc), 0x66 => ("ror", Zp), 0x76 => ("ror", ZpX), 0x6E => ("ror", Abs), 0x7E => ("ror", AbsX),
        _ => ("???", Imp),
    }
}

/// A dictionary entry.
pub struct Word {
    pub name: String,
    pub nt: u16,     // name token (header address)
    pub xt: u16,     // execution token (code address)
    pub flags: u8,
}

const LATEST_LSB: usize = 0x1b5e;
const LATEST_MSB: usize = 0x1b60;

fn petscii_lower(b: u8) -> char {
    match b {
        0x41..=0x5A => (b + 0x20) as char,
        0xC1..=0xDA => (b - 0xC1 + 0x61) as char,
        _ => b as char,
    }
}

/// Walk the dictionary from LATEST upward to the $9fff terminator.
pub fn walk(mem: &[u8]) -> Vec<Word> {
    let mut out = Vec::new();
    let mut w = mem[LATEST_LSB] as u16 | ((mem[LATEST_MSB] as u16) << 8);
    let mut guard = 0;
    loop {
        let lenbyte = mem[w as usize];
        if lenbyte == 0 || guard > 4000 { break; }
        let namelen = (lenbyte & 0x1f) as u16;
        let mut name = String::new();
        for i in 0..namelen {
            name.push(petscii_lower(mem[(w + 1 + i) as usize]));
        }
        let xtpos = (w + 1 + namelen) as usize;
        let xt = mem[xtpos] as u16 | ((mem[xtpos + 1] as u16) << 8);
        out.push(Word { name, nt: w, xt, flags: lenbyte & 0xe0 });
        w = w.wrapping_add(namelen + 3);
        guard += 1;
    }
    out
}

pub fn find<'a>(words: &'a [Word], name: &str) -> Option<&'a Word> {
    words.iter().find(|w| w.name == name)
}

/// Disassemble `n` instructions from `start`, resolving jsr/jmp targets to word
/// names where possible. Returns formatted lines.
pub fn disasm(mem: &[u8], start: u16, n: usize, words: &[Word]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut pc = start;
    for _ in 0..n {
        let op = mem[pc as usize];
        let (mn, mode) = decode(op);
        let len = mode.len();
        let mut bytes = String::new();
        for k in 0..len { bytes += &format!("{:02X} ", mem[(pc + k as u16) as usize]); }
        let operand = match mode {
            Mode::Imp | Mode::Acc => String::new(),
            Mode::Imm => format!("#${:02X}", mem[(pc + 1) as usize]),
            Mode::Zp => format!("${:02X}", mem[(pc + 1) as usize]),
            Mode::ZpX => format!("${:02X},x", mem[(pc + 1) as usize]),
            Mode::ZpY => format!("${:02X},y", mem[(pc + 1) as usize]),
            Mode::IndX => format!("(${:02X},x)", mem[(pc + 1) as usize]),
            Mode::IndY => format!("(${:02X}),y", mem[(pc + 1) as usize]),
            Mode::Rel => {
                let off = mem[(pc + 1) as usize] as i8 as i32;
                format!("${:04X}", (pc as i32 + 2 + off) as u16)
            }
            Mode::Abs | Mode::AbsX | Mode::AbsY | Mode::Ind => {
                let a = mem[(pc + 1) as usize] as u16 | ((mem[(pc + 2) as usize] as u16) << 8);
                let sfx = match mode { Mode::AbsX => ",x", Mode::AbsY => ",y", Mode::Ind => " (ind)", _ => "" };
                let nm = words.iter().find(|w| w.xt == a).map(|w| format!(" <{}>", w.name)).unwrap_or_default();
                format!("${:04X}{}{}", a, sfx, nm)
            }
        };
        lines.push(format!("  {:04X}: {:<9} {} {}", pc, bytes.trim_end(), mn, operand));
        if mn == "rts" || mn == "rti" { break; }
        pc += len as u16;
    }
    lines
}
