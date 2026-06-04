//! Cycle-exact NMOS 6502 / 6510 core.
//!
//! Cycle model is the canonical NMOS table: base cycles per opcode, +1 for a
//! page-cross on indexed *reads* (`abs,X` / `abs,Y` / `(zp),Y`), +1 for a taken
//! branch and +1 more if it crosses a page. Stores and read-modify-write take
//! their fixed counts. Zero-page,X/Y wrap within the zero page and never cross.
//! Decimal mode is implemented for completeness (ChronoForth math does not use
//! it, but BCD correctness is cheap insurance).

pub const C: u8 = 0x01;
pub const Z: u8 = 0x02;
pub const I: u8 = 0x04;
pub const D: u8 = 0x08;
pub const B: u8 = 0x10;
pub const U: u8 = 0x20; // unused, always 1
pub const V: u8 = 0x40;
pub const N: u8 = 0x80;

/// Memory + I/O interface. Implementors back RAM and any memory-mapped stubs.
pub trait Bus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
}

/// Flat 64 KiB RAM — enough for self-contained stack/math primitives.
pub struct Ram {
    pub mem: Box<[u8; 65536]>,
}
impl Default for Ram {
    fn default() -> Self {
        Ram { mem: Box::new([0u8; 65536]) }
    }
}
impl Bus for Ram {
    fn read(&mut self, addr: u16) -> u8 {
        self.mem[addr as usize]
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.mem[addr as usize] = val;
    }
}

#[derive(Debug)]
pub struct CpuError(pub String);

pub struct Cpu<B: Bus> {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub pc: u16,
    pub p: u8,
    pub cycles: u64,
    pub bus: B,
}

impl<B: Bus> Cpu<B> {
    pub fn new(bus: B) -> Self {
        Cpu { a: 0, x: 0, y: 0, sp: 0xFD, pc: 0, p: U | I, cycles: 0, bus }
    }

    // ---- memory helpers ----
    #[inline]
    fn rb(&mut self, a: u16) -> u8 {
        self.bus.read(a)
    }
    #[inline]
    fn wb(&mut self, a: u16, v: u8) {
        self.bus.write(a, v);
    }
    #[inline]
    fn rw(&mut self, a: u16) -> u16 {
        let lo = self.rb(a) as u16;
        let hi = self.rb(a.wrapping_add(1)) as u16;
        lo | (hi << 8)
    }
    #[inline]
    fn fb(&mut self) -> u8 {
        let v = self.rb(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }
    #[inline]
    fn fw(&mut self) -> u16 {
        let lo = self.fb() as u16;
        let hi = self.fb() as u16;
        lo | (hi << 8)
    }

    // ---- stack ----
    #[inline]
    fn push(&mut self, v: u8) {
        self.wb(0x100 | self.sp as u16, v);
        self.sp = self.sp.wrapping_sub(1);
    }
    #[inline]
    fn pop(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.rb(0x100 | self.sp as u16)
    }
    #[inline]
    pub fn pushw(&mut self, v: u16) {
        self.push((v >> 8) as u8);
        self.push((v & 0xFF) as u8);
    }
    #[inline]
    pub fn popw(&mut self) -> u16 {
        let lo = self.pop() as u16;
        let hi = self.pop() as u16;
        lo | (hi << 8)
    }

    // ---- flags ----
    #[inline]
    fn set_flag(&mut self, f: u8, on: bool) {
        if on { self.p |= f } else { self.p &= !f }
    }
    #[inline]
    fn setzn(&mut self, v: u8) {
        self.set_flag(Z, v == 0);
        self.set_flag(N, v & 0x80 != 0);
    }

    // ---- addressing modes -> (effective addr, page_crossed) ----
    fn am_zp(&mut self) -> u16 {
        self.fb() as u16
    }
    fn am_zpx(&mut self) -> u16 {
        self.fb().wrapping_add(self.x) as u16
    }
    fn am_zpy(&mut self) -> u16 {
        self.fb().wrapping_add(self.y) as u16
    }
    fn am_abs(&mut self) -> u16 {
        self.fw()
    }
    fn am_absx(&mut self) -> (u16, bool) {
        let base = self.fw();
        let a = base.wrapping_add(self.x as u16);
        (a, (base & 0xFF00) != (a & 0xFF00))
    }
    fn am_absy(&mut self) -> (u16, bool) {
        let base = self.fw();
        let a = base.wrapping_add(self.y as u16);
        (a, (base & 0xFF00) != (a & 0xFF00))
    }
    fn am_indx(&mut self) -> u16 {
        let zp = self.fb().wrapping_add(self.x);
        let lo = self.rb(zp as u16) as u16;
        let hi = self.rb(zp.wrapping_add(1) as u16) as u16;
        lo | (hi << 8)
    }
    fn am_indy(&mut self) -> (u16, bool) {
        let zp = self.fb();
        let lo = self.rb(zp as u16) as u16;
        let hi = self.rb(zp.wrapping_add(1) as u16) as u16;
        let base = lo | (hi << 8);
        let a = base.wrapping_add(self.y as u16);
        (a, (base & 0xFF00) != (a & 0xFF00))
    }
    fn am_ind(&mut self) -> u16 {
        // JMP (abs) with NMOS page-boundary bug
        let ptr = self.fw();
        let lo = self.rb(ptr) as u16;
        let hi = self.rb((ptr & 0xFF00) | (ptr.wrapping_add(1) & 0x00FF)) as u16;
        lo | (hi << 8)
    }

    fn branch(&mut self, cond: bool) {
        let off = self.fb() as i8 as i16;
        if cond {
            let t = (self.pc as i16).wrapping_add(off) as u16;
            self.cycles += 1;
            if (t & 0xFF00) != (self.pc & 0xFF00) {
                self.cycles += 1;
            }
            self.pc = t;
        }
    }

    fn adc(&mut self, m: u8) {
        if self.p & D != 0 {
            let cin = (self.p & C) as u16;
            let a = self.a as u16;
            let mm = m as u16;
            let mut lo = (a & 0x0F) + (mm & 0x0F) + cin;
            let mut hi = (a >> 4) + (mm >> 4) + if lo > 0x0F { 1 } else { 0 };
            if lo > 9 { lo += 6; }
            let bin = (a + mm + cin) & 0xFF;
            self.set_flag(Z, bin == 0);
            self.set_flag(N, (hi << 4) & 0x80 != 0);
            let ov = (!(a ^ mm) & (a ^ (hi << 4)) & 0x80) != 0;
            self.set_flag(V, ov);
            if hi > 9 { hi += 6; }
            self.set_flag(C, hi > 0x0F);
            self.a = (((hi << 4) | (lo & 0x0F)) & 0xFF) as u8;
        } else {
            let s = self.a as u16 + m as u16 + (self.p & C) as u16;
            let r = (s & 0xFF) as u8;
            self.set_flag(C, s > 0xFF);
            let ov = (!(self.a ^ m) & (self.a ^ r) & 0x80) != 0;
            self.set_flag(V, ov);
            self.a = r;
            self.setzn(r);
        }
    }
    fn sbc(&mut self, m: u8) {
        if self.p & D != 0 {
            let cin = (self.p & C) as i16;
            let a = self.a as i16;
            let mm = m as i16;
            let mut lo = (a & 0x0F) - (mm & 0x0F) - (1 - cin);
            let mut hi = (a >> 4) - (mm >> 4) - if lo < 0 { 1 } else { 0 };
            if lo < 0 { lo += 10; }
            if hi < 0 { hi += 10; }
            let s = a - mm - (1 - cin);
            self.set_flag(C, s >= 0);
            let r = (s & 0xFF) as u8;
            let ov = ((self.a ^ m) & (self.a ^ r) & 0x80) != 0;
            self.set_flag(V, ov);
            self.a = (((hi << 4) | (lo & 0x0F)) & 0xFF) as u8;
            self.setzn(r);
        } else {
            self.adc(m ^ 0xFF);
        }
    }
    fn cmp_reg(&mut self, r: u8, m: u8) {
        self.set_flag(C, r >= m);
        let d = r.wrapping_sub(m);
        self.set_flag(Z, r == m);
        self.set_flag(N, d & 0x80 != 0);
    }
    fn asl(&mut self, v: u8) -> u8 {
        self.set_flag(C, v & 0x80 != 0);
        let r = v << 1;
        self.setzn(r);
        r
    }
    fn lsr(&mut self, v: u8) -> u8 {
        self.set_flag(C, v & 1 != 0);
        let r = v >> 1;
        self.setzn(r);
        r
    }
    fn rol(&mut self, v: u8) -> u8 {
        let cin = self.p & C;
        self.set_flag(C, v & 0x80 != 0);
        let r = (v << 1) | cin;
        self.setzn(r);
        r
    }
    fn ror(&mut self, v: u8) -> u8 {
        let cin = (self.p & C) << 7;
        self.set_flag(C, v & 1 != 0);
        let r = (v >> 1) | cin;
        self.setzn(r);
        r
    }

    /// Execute one instruction; returns Err on an unimplemented opcode.
    pub fn step(&mut self) -> Result<(), CpuError> {
        let op = self.fb();
        match op {
            // ---- LDA ----
            0xA9 => { self.cycles += 2; let a = self.pc; self.pc = self.pc.wrapping_add(1); self.a = self.rb(a); let v=self.a; self.setzn(v); }
            0xA5 => { self.cycles += 3; let m = self.am_zp();  self.a = self.rb(m); let v=self.a; self.setzn(v); }
            0xB5 => { self.cycles += 4; let m = self.am_zpx(); self.a = self.rb(m); let v=self.a; self.setzn(v); }
            0xAD => { self.cycles += 4; let m = self.am_abs(); self.a = self.rb(m); let v=self.a; self.setzn(v); }
            0xBD => { self.cycles += 4; let (m,c)=self.am_absx(); self.a=self.rb(m); let v=self.a; self.setzn(v); if c {self.cycles+=1;} }
            0xB9 => { self.cycles += 4; let (m,c)=self.am_absy(); self.a=self.rb(m); let v=self.a; self.setzn(v); if c {self.cycles+=1;} }
            0xA1 => { self.cycles += 6; let m=self.am_indx(); self.a=self.rb(m); let v=self.a; self.setzn(v); }
            0xB1 => { self.cycles += 5; let (m,c)=self.am_indy(); self.a=self.rb(m); let v=self.a; self.setzn(v); if c {self.cycles+=1;} }
            // ---- LDX ----
            0xA2 => { self.cycles += 2; let a=self.pc; self.pc=self.pc.wrapping_add(1); self.x=self.rb(a); let v=self.x; self.setzn(v); }
            0xA6 => { self.cycles += 3; let m=self.am_zp();  self.x=self.rb(m); let v=self.x; self.setzn(v); }
            0xB6 => { self.cycles += 4; let m=self.am_zpy(); self.x=self.rb(m); let v=self.x; self.setzn(v); }
            0xAE => { self.cycles += 4; let m=self.am_abs(); self.x=self.rb(m); let v=self.x; self.setzn(v); }
            0xBE => { self.cycles += 4; let (m,c)=self.am_absy(); self.x=self.rb(m); let v=self.x; self.setzn(v); if c {self.cycles+=1;} }
            // ---- LDY ----
            0xA0 => { self.cycles += 2; let a=self.pc; self.pc=self.pc.wrapping_add(1); self.y=self.rb(a); let v=self.y; self.setzn(v); }
            0xA4 => { self.cycles += 3; let m=self.am_zp();  self.y=self.rb(m); let v=self.y; self.setzn(v); }
            0xB4 => { self.cycles += 4; let m=self.am_zpx(); self.y=self.rb(m); let v=self.y; self.setzn(v); }
            0xAC => { self.cycles += 4; let m=self.am_abs(); self.y=self.rb(m); let v=self.y; self.setzn(v); }
            0xBC => { self.cycles += 4; let (m,c)=self.am_absx(); self.y=self.rb(m); let v=self.y; self.setzn(v); if c {self.cycles+=1;} }
            // ---- STA ----
            0x85 => { self.cycles += 3; let m=self.am_zp();  let a=self.a; self.wb(m,a); }
            0x95 => { self.cycles += 4; let m=self.am_zpx(); let a=self.a; self.wb(m,a); }
            0x8D => { self.cycles += 4; let m=self.am_abs(); let a=self.a; self.wb(m,a); }
            0x9D => { self.cycles += 5; let (m,_)=self.am_absx(); let a=self.a; self.wb(m,a); }
            0x99 => { self.cycles += 5; let (m,_)=self.am_absy(); let a=self.a; self.wb(m,a); }
            0x81 => { self.cycles += 6; let m=self.am_indx(); let a=self.a; self.wb(m,a); }
            0x91 => { self.cycles += 6; let (m,_)=self.am_indy(); let a=self.a; self.wb(m,a); }
            // ---- STX/STY ----
            0x86 => { self.cycles += 3; let m=self.am_zp();  let v=self.x; self.wb(m,v); }
            0x96 => { self.cycles += 4; let m=self.am_zpy(); let v=self.x; self.wb(m,v); }
            0x8E => { self.cycles += 4; let m=self.am_abs(); let v=self.x; self.wb(m,v); }
            0x84 => { self.cycles += 3; let m=self.am_zp();  let v=self.y; self.wb(m,v); }
            0x94 => { self.cycles += 4; let m=self.am_zpx(); let v=self.y; self.wb(m,v); }
            0x8C => { self.cycles += 4; let m=self.am_abs(); let v=self.y; self.wb(m,v); }
            // ---- transfers ----
            0xAA => { self.cycles += 2; self.x=self.a; let v=self.x; self.setzn(v); }
            0xA8 => { self.cycles += 2; self.y=self.a; let v=self.y; self.setzn(v); }
            0x8A => { self.cycles += 2; self.a=self.x; let v=self.a; self.setzn(v); }
            0x98 => { self.cycles += 2; self.a=self.y; let v=self.a; self.setzn(v); }
            0xBA => { self.cycles += 2; self.x=self.sp; let v=self.x; self.setzn(v); }
            0x9A => { self.cycles += 2; self.sp=self.x; }
            // ---- stack ops ----
            0x48 => { self.cycles += 3; let v=self.a; self.push(v); }
            0x68 => { self.cycles += 4; self.a=self.pop(); let v=self.a; self.setzn(v); }
            0x08 => { self.cycles += 3; let v=self.p|B|U; self.push(v); }
            0x28 => { self.cycles += 4; self.p=(self.pop() & !B)|U; }
            // ---- AND/ORA/EOR ----
            0x29 => { self.cycles += 2; let a=self.pc; self.pc=self.pc.wrapping_add(1); let m=self.rb(a); self.a&=m; let v=self.a; self.setzn(v); }
            0x25 => { self.cycles += 3; let m=self.am_zp();  let x=self.rb(m); self.a&=x; let v=self.a; self.setzn(v); }
            0x35 => { self.cycles += 4; let m=self.am_zpx(); let x=self.rb(m); self.a&=x; let v=self.a; self.setzn(v); }
            0x2D => { self.cycles += 4; let m=self.am_abs(); let x=self.rb(m); self.a&=x; let v=self.a; self.setzn(v); }
            0x3D => { self.cycles += 4; let (m,c)=self.am_absx(); let x=self.rb(m); self.a&=x; let v=self.a; self.setzn(v); if c {self.cycles+=1;} }
            0x39 => { self.cycles += 4; let (m,c)=self.am_absy(); let x=self.rb(m); self.a&=x; let v=self.a; self.setzn(v); if c {self.cycles+=1;} }
            0x21 => { self.cycles += 6; let m=self.am_indx(); let x=self.rb(m); self.a&=x; let v=self.a; self.setzn(v); }
            0x31 => { self.cycles += 5; let (m,c)=self.am_indy(); let x=self.rb(m); self.a&=x; let v=self.a; self.setzn(v); if c {self.cycles+=1;} }
            0x09 => { self.cycles += 2; let a=self.pc; self.pc=self.pc.wrapping_add(1); let m=self.rb(a); self.a|=m; let v=self.a; self.setzn(v); }
            0x05 => { self.cycles += 3; let m=self.am_zp();  let x=self.rb(m); self.a|=x; let v=self.a; self.setzn(v); }
            0x15 => { self.cycles += 4; let m=self.am_zpx(); let x=self.rb(m); self.a|=x; let v=self.a; self.setzn(v); }
            0x0D => { self.cycles += 4; let m=self.am_abs(); let x=self.rb(m); self.a|=x; let v=self.a; self.setzn(v); }
            0x1D => { self.cycles += 4; let (m,c)=self.am_absx(); let x=self.rb(m); self.a|=x; let v=self.a; self.setzn(v); if c {self.cycles+=1;} }
            0x19 => { self.cycles += 4; let (m,c)=self.am_absy(); let x=self.rb(m); self.a|=x; let v=self.a; self.setzn(v); if c {self.cycles+=1;} }
            0x01 => { self.cycles += 6; let m=self.am_indx(); let x=self.rb(m); self.a|=x; let v=self.a; self.setzn(v); }
            0x11 => { self.cycles += 5; let (m,c)=self.am_indy(); let x=self.rb(m); self.a|=x; let v=self.a; self.setzn(v); if c {self.cycles+=1;} }
            0x49 => { self.cycles += 2; let a=self.pc; self.pc=self.pc.wrapping_add(1); let m=self.rb(a); self.a^=m; let v=self.a; self.setzn(v); }
            0x45 => { self.cycles += 3; let m=self.am_zp();  let x=self.rb(m); self.a^=x; let v=self.a; self.setzn(v); }
            0x55 => { self.cycles += 4; let m=self.am_zpx(); let x=self.rb(m); self.a^=x; let v=self.a; self.setzn(v); }
            0x4D => { self.cycles += 4; let m=self.am_abs(); let x=self.rb(m); self.a^=x; let v=self.a; self.setzn(v); }
            0x5D => { self.cycles += 4; let (m,c)=self.am_absx(); let x=self.rb(m); self.a^=x; let v=self.a; self.setzn(v); if c {self.cycles+=1;} }
            0x59 => { self.cycles += 4; let (m,c)=self.am_absy(); let x=self.rb(m); self.a^=x; let v=self.a; self.setzn(v); if c {self.cycles+=1;} }
            0x41 => { self.cycles += 6; let m=self.am_indx(); let x=self.rb(m); self.a^=x; let v=self.a; self.setzn(v); }
            0x51 => { self.cycles += 5; let (m,c)=self.am_indy(); let x=self.rb(m); self.a^=x; let v=self.a; self.setzn(v); if c {self.cycles+=1;} }
            // ---- BIT ----
            0x24 => { self.cycles += 3; let m=self.am_zp();  let v=self.rb(m); self.set_flag(Z,(self.a&v)==0); self.set_flag(N,v&0x80!=0); self.set_flag(V,v&0x40!=0); }
            0x2C => { self.cycles += 4; let m=self.am_abs(); let v=self.rb(m); self.set_flag(Z,(self.a&v)==0); self.set_flag(N,v&0x80!=0); self.set_flag(V,v&0x40!=0); }
            // ---- ADC ----
            0x69 => { self.cycles += 2; let a=self.pc; self.pc=self.pc.wrapping_add(1); let m=self.rb(a); self.adc(m); }
            0x65 => { self.cycles += 3; let m=self.am_zp();  let v=self.rb(m); self.adc(v); }
            0x75 => { self.cycles += 4; let m=self.am_zpx(); let v=self.rb(m); self.adc(v); }
            0x6D => { self.cycles += 4; let m=self.am_abs(); let v=self.rb(m); self.adc(v); }
            0x7D => { self.cycles += 4; let (m,c)=self.am_absx(); let v=self.rb(m); self.adc(v); if c {self.cycles+=1;} }
            0x79 => { self.cycles += 4; let (m,c)=self.am_absy(); let v=self.rb(m); self.adc(v); if c {self.cycles+=1;} }
            0x61 => { self.cycles += 6; let m=self.am_indx(); let v=self.rb(m); self.adc(v); }
            0x71 => { self.cycles += 5; let (m,c)=self.am_indy(); let v=self.rb(m); self.adc(v); if c {self.cycles+=1;} }
            // ---- SBC ----
            0xE9 => { self.cycles += 2; let a=self.pc; self.pc=self.pc.wrapping_add(1); let m=self.rb(a); self.sbc(m); }
            0xE5 => { self.cycles += 3; let m=self.am_zp();  let v=self.rb(m); self.sbc(v); }
            0xF5 => { self.cycles += 4; let m=self.am_zpx(); let v=self.rb(m); self.sbc(v); }
            0xED => { self.cycles += 4; let m=self.am_abs(); let v=self.rb(m); self.sbc(v); }
            0xFD => { self.cycles += 4; let (m,c)=self.am_absx(); let v=self.rb(m); self.sbc(v); if c {self.cycles+=1;} }
            0xF9 => { self.cycles += 4; let (m,c)=self.am_absy(); let v=self.rb(m); self.sbc(v); if c {self.cycles+=1;} }
            0xE1 => { self.cycles += 6; let m=self.am_indx(); let v=self.rb(m); self.sbc(v); }
            0xF1 => { self.cycles += 5; let (m,c)=self.am_indy(); let v=self.rb(m); self.sbc(v); if c {self.cycles+=1;} }
            // ---- CMP/CPX/CPY ----
            0xC9 => { self.cycles += 2; let a=self.pc; self.pc=self.pc.wrapping_add(1); let m=self.rb(a); let r=self.a; self.cmp_reg(r,m); }
            0xC5 => { self.cycles += 3; let m=self.am_zp();  let v=self.rb(m); let r=self.a; self.cmp_reg(r,v); }
            0xD5 => { self.cycles += 4; let m=self.am_zpx(); let v=self.rb(m); let r=self.a; self.cmp_reg(r,v); }
            0xCD => { self.cycles += 4; let m=self.am_abs(); let v=self.rb(m); let r=self.a; self.cmp_reg(r,v); }
            0xDD => { self.cycles += 4; let (m,c)=self.am_absx(); let v=self.rb(m); let r=self.a; self.cmp_reg(r,v); if c {self.cycles+=1;} }
            0xD9 => { self.cycles += 4; let (m,c)=self.am_absy(); let v=self.rb(m); let r=self.a; self.cmp_reg(r,v); if c {self.cycles+=1;} }
            0xC1 => { self.cycles += 6; let m=self.am_indx(); let v=self.rb(m); let r=self.a; self.cmp_reg(r,v); }
            0xD1 => { self.cycles += 5; let (m,c)=self.am_indy(); let v=self.rb(m); let r=self.a; self.cmp_reg(r,v); if c {self.cycles+=1;} }
            0xE0 => { self.cycles += 2; let a=self.pc; self.pc=self.pc.wrapping_add(1); let m=self.rb(a); let r=self.x; self.cmp_reg(r,m); }
            0xE4 => { self.cycles += 3; let m=self.am_zp();  let v=self.rb(m); let r=self.x; self.cmp_reg(r,v); }
            0xEC => { self.cycles += 4; let m=self.am_abs(); let v=self.rb(m); let r=self.x; self.cmp_reg(r,v); }
            0xC0 => { self.cycles += 2; let a=self.pc; self.pc=self.pc.wrapping_add(1); let m=self.rb(a); let r=self.y; self.cmp_reg(r,m); }
            0xC4 => { self.cycles += 3; let m=self.am_zp();  let v=self.rb(m); let r=self.y; self.cmp_reg(r,v); }
            0xCC => { self.cycles += 4; let m=self.am_abs(); let v=self.rb(m); let r=self.y; self.cmp_reg(r,v); }
            // ---- INC/DEC mem ----
            0xE6 => { self.cycles += 5; let m=self.am_zp();  let v=self.rb(m).wrapping_add(1); self.wb(m,v); self.setzn(v); }
            0xF6 => { self.cycles += 6; let m=self.am_zpx(); let v=self.rb(m).wrapping_add(1); self.wb(m,v); self.setzn(v); }
            0xEE => { self.cycles += 6; let m=self.am_abs(); let v=self.rb(m).wrapping_add(1); self.wb(m,v); self.setzn(v); }
            0xFE => { self.cycles += 7; let (m,_)=self.am_absx(); let v=self.rb(m).wrapping_add(1); self.wb(m,v); self.setzn(v); }
            0xC6 => { self.cycles += 5; let m=self.am_zp();  let v=self.rb(m).wrapping_sub(1); self.wb(m,v); self.setzn(v); }
            0xD6 => { self.cycles += 6; let m=self.am_zpx(); let v=self.rb(m).wrapping_sub(1); self.wb(m,v); self.setzn(v); }
            0xCE => { self.cycles += 6; let m=self.am_abs(); let v=self.rb(m).wrapping_sub(1); self.wb(m,v); self.setzn(v); }
            0xDE => { self.cycles += 7; let (m,_)=self.am_absx(); let v=self.rb(m).wrapping_sub(1); self.wb(m,v); self.setzn(v); }
            0xE8 => { self.cycles += 2; self.x=self.x.wrapping_add(1); let v=self.x; self.setzn(v); }
            0xC8 => { self.cycles += 2; self.y=self.y.wrapping_add(1); let v=self.y; self.setzn(v); }
            0xCA => { self.cycles += 2; self.x=self.x.wrapping_sub(1); let v=self.x; self.setzn(v); }
            0x88 => { self.cycles += 2; self.y=self.y.wrapping_sub(1); let v=self.y; self.setzn(v); }
            // ---- shifts (accumulator) ----
            0x0A => { self.cycles += 2; let v=self.a; self.a=self.asl(v); }
            0x4A => { self.cycles += 2; let v=self.a; self.a=self.lsr(v); }
            0x2A => { self.cycles += 2; let v=self.a; self.a=self.rol(v); }
            0x6A => { self.cycles += 2; let v=self.a; self.a=self.ror(v); }
            // ---- shifts (memory) ----
            0x06 => { self.cycles += 5; let m=self.am_zp();  let v=self.rb(m); let r=self.asl(v); self.wb(m,r); }
            0x16 => { self.cycles += 6; let m=self.am_zpx(); let v=self.rb(m); let r=self.asl(v); self.wb(m,r); }
            0x0E => { self.cycles += 6; let m=self.am_abs(); let v=self.rb(m); let r=self.asl(v); self.wb(m,r); }
            0x1E => { self.cycles += 7; let (m,_)=self.am_absx(); let v=self.rb(m); let r=self.asl(v); self.wb(m,r); }
            0x46 => { self.cycles += 5; let m=self.am_zp();  let v=self.rb(m); let r=self.lsr(v); self.wb(m,r); }
            0x56 => { self.cycles += 6; let m=self.am_zpx(); let v=self.rb(m); let r=self.lsr(v); self.wb(m,r); }
            0x4E => { self.cycles += 6; let m=self.am_abs(); let v=self.rb(m); let r=self.lsr(v); self.wb(m,r); }
            0x5E => { self.cycles += 7; let (m,_)=self.am_absx(); let v=self.rb(m); let r=self.lsr(v); self.wb(m,r); }
            0x26 => { self.cycles += 5; let m=self.am_zp();  let v=self.rb(m); let r=self.rol(v); self.wb(m,r); }
            0x36 => { self.cycles += 6; let m=self.am_zpx(); let v=self.rb(m); let r=self.rol(v); self.wb(m,r); }
            0x2E => { self.cycles += 6; let m=self.am_abs(); let v=self.rb(m); let r=self.rol(v); self.wb(m,r); }
            0x3E => { self.cycles += 7; let (m,_)=self.am_absx(); let v=self.rb(m); let r=self.rol(v); self.wb(m,r); }
            0x66 => { self.cycles += 5; let m=self.am_zp();  let v=self.rb(m); let r=self.ror(v); self.wb(m,r); }
            0x76 => { self.cycles += 6; let m=self.am_zpx(); let v=self.rb(m); let r=self.ror(v); self.wb(m,r); }
            0x6E => { self.cycles += 6; let m=self.am_abs(); let v=self.rb(m); let r=self.ror(v); self.wb(m,r); }
            0x7E => { self.cycles += 7; let (m,_)=self.am_absx(); let v=self.rb(m); let r=self.ror(v); self.wb(m,r); }
            // ---- jumps / calls / returns ----
            0x4C => { self.cycles += 3; self.pc=self.am_abs(); }
            0x6C => { self.cycles += 5; self.pc=self.am_ind(); }
            0x20 => { self.cycles += 6; let addr=self.fw(); let ret=self.pc.wrapping_sub(1); self.pushw(ret); self.pc=addr; }
            0x60 => { self.cycles += 6; self.pc=self.popw().wrapping_add(1); }
            0x40 => { self.cycles += 6; self.p=(self.pop() & !B)|U; self.pc=self.popw(); }
            0x00 => { self.cycles += 7; self.pc=self.pc.wrapping_add(1); let pc=self.pc; self.pushw(pc); let pp=self.p|B|U; self.push(pp); self.p|=I; self.pc=self.rw(0xFFFE); }
            // ---- branches ----
            0x10 => { self.cycles += 2; let c=self.p&N==0; self.branch(c); }
            0x30 => { self.cycles += 2; let c=self.p&N!=0; self.branch(c); }
            0x50 => { self.cycles += 2; let c=self.p&V==0; self.branch(c); }
            0x70 => { self.cycles += 2; let c=self.p&V!=0; self.branch(c); }
            0x90 => { self.cycles += 2; let c=self.p&C==0; self.branch(c); }
            0xB0 => { self.cycles += 2; let c=self.p&C!=0; self.branch(c); }
            0xD0 => { self.cycles += 2; let c=self.p&Z==0; self.branch(c); }
            0xF0 => { self.cycles += 2; let c=self.p&Z!=0; self.branch(c); }
            // ---- flags / nop ----
            0x18 => { self.cycles += 2; self.p&=!C; }
            0x38 => { self.cycles += 2; self.p|=C; }
            0x58 => { self.cycles += 2; self.p&=!I; }
            0x78 => { self.cycles += 2; self.p|=I; }
            0xB8 => { self.cycles += 2; self.p&=!V; }
            0xD8 => { self.cycles += 2; self.p&=!D; }
            0xF8 => { self.cycles += 2; self.p|=D; }
            0xEA => { self.cycles += 2; }
            other => return Err(CpuError(format!(
                "unimplemented opcode ${:02X} at ${:04X}", other, self.pc.wrapping_sub(1)))),
        }
        Ok(())
    }

    /// Run until PC == stop, or error / runaway. Returns cycles elapsed.
    pub fn run_until(&mut self, stop: u16, max_cycles: u64) -> Result<u64, CpuError> {
        let start = self.cycles;
        while self.pc != stop {
            self.step()?;
            if self.cycles - start > max_cycles {
                return Err(CpuError(format!(
                    "runaway: > {} cycles, pc=${:04X}", max_cycles, self.pc)));
            }
        }
        Ok(self.cycles - start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Load `prog` at $0200, run `steps` instructions, return the cpu.
    fn run(prog: &[u8], steps: usize) -> Cpu<Ram> {
        let mut cpu = Cpu::new(Ram::default());
        for (i, b) in prog.iter().enumerate() {
            cpu.bus.mem[0x0200 + i] = *b;
        }
        cpu.pc = 0x0200;
        for _ in 0..steps {
            cpu.step().unwrap();
        }
        cpu
    }

    #[test]
    fn imm_and_zp_cycles() {
        // LDA #$05 (2)  STA $10 (3)
        let c = run(&[0xA9, 0x05, 0x85, 0x10], 2);
        assert_eq!(c.a, 0x05);
        assert_eq!(c.bus.mem[0x10], 0x05);
        assert_eq!(c.cycles, 5);
    }

    #[test]
    fn absx_page_cross_penalty() {
        // LDA $12FF,X with X=1 crosses into $1300 -> 4+1 = 5 cycles
        let mut cpu = Cpu::new(Ram::default());
        cpu.bus.mem[0x1300] = 0x42;
        cpu.x = 1;
        cpu.bus.mem[0x0200..0x0203].copy_from_slice(&[0xBD, 0xFF, 0x12]);
        cpu.pc = 0x0200;
        cpu.step().unwrap();
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.cycles, 5); // 4 base + 1 page-cross
    }

    #[test]
    fn absx_no_cross() {
        // LDA $1300,X with X=1 -> no cross -> 4 cycles
        let mut cpu = Cpu::new(Ram::default());
        cpu.bus.mem[0x1301] = 0x42;
        cpu.x = 1;
        cpu.bus.mem[0x0200..0x0203].copy_from_slice(&[0xBD, 0x00, 0x13]);
        cpu.pc = 0x0200;
        cpu.step().unwrap();
        assert_eq!(cpu.cycles, 4);
    }

    #[test]
    fn sta_absx_no_penalty() {
        // STA $12FF,X X=1 -> always 5 cycles (stores get no page-cross bonus)
        let mut cpu = Cpu::new(Ram::default());
        cpu.x = 1;
        cpu.a = 0x99;
        cpu.bus.mem[0x0200..0x0203].copy_from_slice(&[0x9D, 0xFF, 0x12]);
        cpu.pc = 0x0200;
        cpu.step().unwrap();
        assert_eq!(cpu.bus.mem[0x1300], 0x99);
        assert_eq!(cpu.cycles, 5);
    }

    #[test]
    fn branch_taken_and_cross() {
        // not taken: BEQ +2 with Z=0 -> 2 cycles
        let mut c = Cpu::new(Ram::default());
        c.p &= !Z;
        c.bus.mem[0x0200..0x0202].copy_from_slice(&[0xF0, 0x02]);
        c.pc = 0x0200;
        c.step().unwrap();
        assert_eq!(c.cycles, 2);

        // taken, same page -> 3 cycles
        let mut c = Cpu::new(Ram::default());
        c.p |= Z;
        c.bus.mem[0x0200..0x0202].copy_from_slice(&[0xF0, 0x02]);
        c.pc = 0x0200;
        c.step().unwrap();
        assert_eq!(c.cycles, 3);
        assert_eq!(c.pc, 0x0204);

        // taken, crossing page boundary -> 4 cycles
        let mut c = Cpu::new(Ram::default());
        c.p |= Z;
        c.bus.mem[0x02FD..0x02FF].copy_from_slice(&[0xF0, 0x10]);
        c.pc = 0x02FD;
        c.step().unwrap();
        assert_eq!(c.cycles, 4);
        assert_eq!(c.pc, 0x030F);
    }

    #[test]
    fn adc_overflow_and_carry() {
        // 0x50 + 0x50 = 0xA0: overflow set (pos+pos->neg), carry clear
        let mut c = Cpu::new(Ram::default());
        c.a = 0x50;
        c.bus.mem[0x0200..0x0202].copy_from_slice(&[0x69, 0x50]); // ADC #$50
        c.pc = 0x0200;
        c.step().unwrap();
        assert_eq!(c.a, 0xA0);
        assert!(c.p & V != 0);
        assert!(c.p & C == 0);
        assert!(c.p & N != 0);
    }

    #[test]
    fn sbc_borrow() {
        // 0x05 - 0x06 with carry set: result 0xFF, carry clear (borrow)
        let mut c = Cpu::new(Ram::default());
        c.a = 0x05;
        c.p |= C;
        c.bus.mem[0x0200..0x0202].copy_from_slice(&[0xE9, 0x06]); // SBC #$06
        c.pc = 0x0200;
        c.step().unwrap();
        assert_eq!(c.a, 0xFF);
        assert!(c.p & C == 0);
    }

    #[test]
    fn cmp_sets_carry_when_ge() {
        let mut c = Cpu::new(Ram::default());
        c.a = 0x10;
        c.bus.mem[0x0200..0x0202].copy_from_slice(&[0xC9, 0x05]); // CMP #$05
        c.pc = 0x0200;
        c.step().unwrap();
        assert!(c.p & C != 0); // 0x10 >= 0x05
        assert!(c.p & Z == 0);
    }

    #[test]
    fn jsr_rts_roundtrip_cycles() {
        // JSR $0210 ; (at $0210) RTS  -> 6 + 6 = 12 cycles, pc back past JSR
        let mut c = Cpu::new(Ram::default());
        c.sp = 0xFF;
        c.bus.mem[0x0200..0x0203].copy_from_slice(&[0x20, 0x10, 0x02]);
        c.bus.mem[0x0210] = 0x60; // RTS
        c.pc = 0x0200;
        c.step().unwrap(); // JSR
        assert_eq!(c.pc, 0x0210);
        c.step().unwrap(); // RTS
        assert_eq!(c.pc, 0x0203);
        assert_eq!(c.cycles, 12);
        assert_eq!(c.sp, 0xFF);
    }

    #[test]
    fn zpx_wraps_within_zero_page() {
        // LDA $FF,X with X=2 -> reads $01 (wrap), not $0101
        let mut c = Cpu::new(Ram::default());
        c.bus.mem[0x0001] = 0x77;
        c.bus.mem[0x0101] = 0x33;
        c.x = 2;
        c.bus.mem[0x0200..0x0202].copy_from_slice(&[0xB5, 0xFF]); // LDA $FF,X
        c.pc = 0x0200;
        c.step().unwrap();
        assert_eq!(c.a, 0x77);
        assert_eq!(c.cycles, 4);
    }

    #[test]
    fn indirect_y_page_cross() {
        // LDA ($10),Y : ptr=$12FF, Y=1 -> $1300, crosses -> 5+1 = 6
        let mut c = Cpu::new(Ram::default());
        c.bus.mem[0x10] = 0xFF;
        c.bus.mem[0x11] = 0x12;
        c.bus.mem[0x1300] = 0x5A;
        c.y = 1;
        c.bus.mem[0x0200..0x0202].copy_from_slice(&[0xB1, 0x10]);
        c.pc = 0x0200;
        c.step().unwrap();
        assert_eq!(c.a, 0x5A);
        assert_eq!(c.cycles, 6);
    }
}
