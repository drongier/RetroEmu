//! Outils sur les registres : paires 16 bits, drapeaux, et registres désignés
//! par leur code dans l'opcode.

use super::Cpu;
use crate::bus::Bus;

impl Cpu {
    // --- Paires de registres -------------------------------------------------

    pub(super) fn af(&self) -> u16 {
        ((self.a as u16) << 8) | self.f as u16
    }

    pub(super) fn set_af(&mut self, value: u16) {
        self.a = (value >> 8) as u8;
        self.f = value as u8 & 0xF0; // les 4 bits bas de F n'existent pas
    }

    pub(super) fn bc(&self) -> u16 {
        ((self.b as u16) << 8) | self.c as u16
    }

    pub(super) fn set_bc(&mut self, value: u16) {
        self.b = (value >> 8) as u8;
        self.c = value as u8;
    }

    pub(super) fn de(&self) -> u16 {
        ((self.d as u16) << 8) | self.e as u16
    }

    pub(super) fn set_de(&mut self, value: u16) {
        self.d = (value >> 8) as u8;
        self.e = value as u8;
    }

    pub(super) fn hl(&self) -> u16 {
        ((self.h as u16) << 8) | self.l as u16
    }

    pub(super) fn set_hl(&mut self, value: u16) {
        self.h = (value >> 8) as u8;
        self.l = value as u8;
    }

    // --- Drapeaux ------------------------------------------------------------

    pub(super) fn flag(&self, flag: u8) -> bool {
        self.f & flag != 0
    }

    pub(super) fn set_flag(&mut self, flag: u8, on: bool) {
        if on {
            self.f |= flag;
        } else {
            self.f &= !flag;
        }
    }

    // --- Registres désignés par leur code dans l'opcode -------------------------

    /// Lit le registre 8 bits de code `code` : 0 B, 1 C, 2 D, 3 E, 4 H, 5 L, 6 (HL), 7 A.
    pub(super) fn read_r8(&self, bus: &Bus, code: u8) -> u8 {
        match code {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            6 => bus.read8(self.hl()), // (HL) : la case mémoire pointée par HL
            7 => self.a,
            _ => unreachable!("code de registre 8 bits invalide : {code}"),
        }
    }

    pub(super) fn write_r8(&mut self, bus: &mut Bus, code: u8, value: u8) {
        match code {
            0 => self.b = value,
            1 => self.c = value,
            2 => self.d = value,
            3 => self.e = value,
            4 => self.h = value,
            5 => self.l = value,
            6 => bus.write8(self.hl(), value),
            7 => self.a = value,
            _ => unreachable!("code de registre 8 bits invalide : {code}"),
        }
    }

    /// Lit la paire de code `code` : 0 BC, 1 DE, 2 HL, 3 SP.
    pub(super) fn read_r16(&self, code: u8) -> u16 {
        match code {
            0 => self.bc(),
            1 => self.de(),
            2 => self.hl(),
            3 => self.sp,
            _ => unreachable!("code de paire invalide : {code}"),
        }
    }

    pub(super) fn write_r16(&mut self, code: u8, value: u16) {
        match code {
            0 => self.set_bc(value),
            1 => self.set_de(value),
            2 => self.set_hl(value),
            3 => self.sp = value,
            _ => unreachable!("code de paire invalide : {code}"),
        }
    }

    /// Adresse des LD indirects selon leur code : 0 (BC), 1 (DE), 2 (HL+), 3 (HL-).
    /// HL+ et HL- renvoient HL, puis l'avancent ou le reculent de 1.
    pub(super) fn indirect_addr(&mut self, code: u8) -> u16 {
        match code {
            0 => self.bc(),
            1 => self.de(),
            2 => {
                let hl = self.hl();
                self.set_hl(hl.wrapping_add(1));
                hl
            }
            3 => {
                let hl = self.hl();
                self.set_hl(hl.wrapping_sub(1));
                hl
            }
            _ => unreachable!("code d'adressage indirect invalide : {code}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::bus_with;
    use super::super::{FLAG_C, FLAG_H, FLAG_N, FLAG_Z};
    use super::*;

    #[test]
    fn les_paires_collent_et_decoupent_les_registres() {
        let mut cpu = Cpu::new();
        cpu.set_bc(0x1234);
        assert_eq!((cpu.b, cpu.c), (0x12, 0x34));
        cpu.d = 0xBE;
        cpu.e = 0xEF;
        assert_eq!(cpu.de(), 0xBEEF);
        cpu.set_hl(0xC005);
        assert_eq!(cpu.hl(), 0xC005);
    }

    #[test]
    fn les_bits_bas_de_f_restent_a_zero() {
        let mut cpu = Cpu::new();
        cpu.set_af(0xFFFF);
        assert_eq!(cpu.a, 0xFF);
        assert_eq!(cpu.f, 0xF0);
        assert_eq!(cpu.af(), 0xFFF0);
    }

    #[test]
    fn lever_et_baisser_un_drapeau() {
        let mut cpu = Cpu::new();
        cpu.f = 0;
        cpu.set_flag(FLAG_Z, true);
        cpu.set_flag(FLAG_C, true);
        assert_eq!(cpu.f, 0x90);
        assert!(cpu.flag(FLAG_Z));
        assert!(!cpu.flag(FLAG_N));
        cpu.set_flag(FLAG_Z, false);
        assert_eq!(cpu.f, 0x10);
        assert!(!cpu.flag(FLAG_H));
    }

    #[test]
    fn le_code_6_designe_la_memoire_pointee_par_hl() {
        let mut bus = bus_with(&[]);
        let mut cpu = Cpu::new();
        cpu.set_hl(0xC000);
        cpu.write_r8(&mut bus, 6, 0x42); // (HL) ← 0x42
        assert_eq!(bus.read8(0xC000), 0x42);
        assert_eq!(cpu.read_r8(&bus, 6), 0x42);

        cpu.write_r8(&mut bus, 7, 0x99); // A ← 0x99
        assert_eq!(cpu.a, 0x99);
        assert_eq!(cpu.read_r8(&bus, 7), 0x99);
    }

    #[test]
    fn les_codes_de_paires() {
        let mut cpu = Cpu::new();
        cpu.write_r16(0, 0x1111);
        cpu.write_r16(2, 0x2222);
        cpu.write_r16(3, 0xDFFF);
        assert_eq!(cpu.bc(), 0x1111);
        assert_eq!(cpu.hl(), 0x2222);
        assert_eq!(cpu.sp, 0xDFFF);
        assert_eq!(cpu.read_r16(1), cpu.de());
    }
}
