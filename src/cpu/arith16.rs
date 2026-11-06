//! Calculs sur 16 bits : INC/DEC de paires, ADD HL, rr, et SP + nombre signé.
//!
//! Chaque instruction a ses propres règles de drapeaux, différentes de l'ALU 8 bits.

use super::{Cpu, FLAG_C, FLAG_H, FLAG_N, FLAG_Z};
use crate::bus::Bus;

impl Cpu {
    /// INC rr : 00 pp0 011. Aucun drapeau modifié.
    pub(super) fn inc_rr(&mut self, opcode: u8) -> u32 {
        let code = opcode >> 4;
        let value = self.read_r16(code).wrapping_add(1);
        self.write_r16(code, value);
        8
    }

    /// DEC rr : 00 pp1 011. Aucun drapeau modifié.
    pub(super) fn dec_rr(&mut self, opcode: u8) -> u32 {
        let code = opcode >> 4;
        let value = self.read_r16(code).wrapping_sub(1);
        self.write_r16(code, value);
        8
    }

    /// ADD HL, rr : 00 pp1 001. Z inchangé, H = retenue du bit 11, C = retenue du bit 15.
    pub(super) fn add_hl_rr(&mut self, opcode: u8) -> u32 {
        let hl = self.hl();
        let value = self.read_r16(opcode >> 4);
        let sum = hl as u32 + value as u32;
        self.set_hl(sum as u16);
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_H, (hl & 0x0FFF) + (value & 0x0FFF) > 0x0FFF);
        self.set_flag(FLAG_C, sum > 0xFFFF);
        8
    }

    /// ADD SP, e8
    pub(super) fn add_sp_e8(&mut self, bus: &mut Bus) -> u32 {
        self.sp = self.sp_plus_e8(bus);
        16
    }

    /// LD HL, SP + e8 : SP n'est pas modifié.
    pub(super) fn ld_hl_sp_e8(&mut self, bus: &mut Bus) -> u32 {
        let value = self.sp_plus_e8(bus);
        self.set_hl(value);
        12
    }

    /// Lit un octet signé `e8` et renvoie SP + e8.
    /// Les drapeaux H et C sont ceux de l'addition 8 bits de l'octet bas de SP avec e8.
    fn sp_plus_e8(&mut self, bus: &Bus) -> u16 {
        let byte = self.fetch8(bus);
        let offset = byte as i8 as i16 as u16; // 0xFE → -2 → 0xFFFE
        let sp = self.sp;
        self.set_flag(FLAG_Z, false);
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_H, (sp & 0x0F) + (byte as u16 & 0x0F) > 0x0F);
        self.set_flag(FLAG_C, (sp & 0xFF) + byte as u16 > 0xFF);
        sp.wrapping_add(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::{bus_with, run};
    use super::*;

    #[test]
    fn inc_et_dec_16_bits_ne_touchent_aucun_drapeau() {
        let mut bus = bus_with(&[
            0x01, 0xFF, 0xFF, // LD BC, 0xFFFF
            0x03, //             INC BC   → 0x0000, mais Z ne bouge pas
            0x11, 0x00, 0x00, // LD DE, 0x0000
            0x1B, //             DEC DE   → 0xFFFF
        ]);
        let mut cpu = Cpu::new();
        cpu.f = 0;
        assert_eq!(run(&mut cpu, &mut bus, 4), 12 + 8 + 12 + 8);
        assert_eq!(cpu.bc(), 0x0000);
        assert_eq!(cpu.de(), 0xFFFF);
        assert_eq!(cpu.f, 0);
    }

    #[test]
    fn add_hl_retenue_du_bit_11() {
        let mut bus = bus_with(&[
            0xAF, //             XOR A          → F = Z seul
            0x21, 0xFF, 0x0F, // LD HL, 0x0FFF
            0x01, 0x01, 0x00, // LD BC, 0x0001
            0x09, //             ADD HL, BC
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 4);
        assert_eq!(cpu.hl(), 0x1000);
        assert_eq!(cpu.f, FLAG_Z | FLAG_H); // Z inchangé (hérité du XOR), H levé, pas de C
    }

    #[test]
    fn add_hl_deborde() {
        let mut bus = bus_with(&[
            0x21, 0xFF, 0xFF, // LD HL, 0xFFFF
            0x11, 0x01, 0x00, // LD DE, 0x0001
            0x19, //             ADD HL, DE
        ]);
        let mut cpu = Cpu::new();
        cpu.f = 0;
        run(&mut cpu, &mut bus, 3);
        assert_eq!(cpu.hl(), 0x0000);
        assert_eq!(cpu.f, FLAG_H | FLAG_C); // résultat nul, mais Z n'est pas levé
    }

    #[test]
    fn add_sp_nombre_positif() {
        let mut bus = bus_with(&[
            0x31, 0xF8, 0xFF, // LD SP, 0xFFF8
            0xE8, 0x08, //       ADD SP, +8   (16 tics)
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 1);
        assert_eq!(run(&mut cpu, &mut bus, 1), 16);
        assert_eq!(cpu.sp, 0x0000);
        assert_eq!(cpu.f, FLAG_H | FLAG_C); // 0xF8 + 0x08 déborde sur l'octet bas
    }

    #[test]
    fn add_sp_nombre_negatif() {
        let mut bus = bus_with(&[
            0x31, 0x00, 0xD0, // LD SP, 0xD000
            0xE8, 0xFE, //       ADD SP, -2
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 2);
        assert_eq!(cpu.sp, 0xCFFE);
        assert_eq!(cpu.f, 0); // 0x00 + 0xFE ne déborde pas
    }

    #[test]
    fn ld_hl_sp_plus_e8_ne_modifie_pas_sp() {
        let mut bus = bus_with(&[
            0x31, 0x00, 0xC0, // LD SP, 0xC000
            0xF8, 0x05, //       LD HL, SP + 5   (12 tics)
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 1);
        assert_eq!(run(&mut cpu, &mut bus, 1), 12);
        assert_eq!(cpu.hl(), 0xC005);
        assert_eq!(cpu.sp, 0xC000);
    }
}
