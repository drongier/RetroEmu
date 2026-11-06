//! Les sauts : JP, JR, CALL, RET, RST. Ils déplacent PC, le doigt sur la recette.
//!
//! Les versions conditionnelles testent un drapeau selon leur code cc (bits 4-3 de l'opcode) :
//! 0 NZ (Z = 0), 1 Z (Z = 1), 2 NC (C = 0), 3 C (C = 1).
//! Une adresse ou une distance est toujours lue, même quand on ne saute pas.

use super::{Cpu, FLAG_C, FLAG_Z};
use crate::bus::Bus;

impl Cpu {
    /// Teste la condition cc de l'opcode.
    fn condition(&self, opcode: u8) -> bool {
        match (opcode >> 3) & 0x03 {
            0 => !self.flag(FLAG_Z),
            1 => self.flag(FLAG_Z),
            2 => !self.flag(FLAG_C),
            3 => self.flag(FLAG_C),
            _ => unreachable!(),
        }
    }

    /// Lit une distance signée e8 et renvoie l'adresse d'arrivée, comptée depuis la fin du JR.
    fn jr_target(&mut self, bus: &Bus) -> u16 {
        let offset = self.fetch8(bus) as i8 as i16 as u16;
        self.pc.wrapping_add(offset)
    }

    /// JP n16
    pub(super) fn jp(&mut self, bus: &Bus) -> u32 {
        self.pc = self.fetch16(bus);
        16
    }

    /// JP cc, n16 : 110 cc 010
    pub(super) fn jp_cc(&mut self, bus: &Bus, opcode: u8) -> u32 {
        let addr = self.fetch16(bus);
        if self.condition(opcode) {
            self.pc = addr;
            16
        } else {
            12
        }
    }

    /// JP HL
    pub(super) fn jp_hl(&mut self) -> u32 {
        self.pc = self.hl();
        4
    }

    /// JR e8 : saut relatif
    pub(super) fn jr(&mut self, bus: &Bus) -> u32 {
        self.pc = self.jr_target(bus);
        12
    }

    /// JR cc, e8 : 001 cc 000
    pub(super) fn jr_cc(&mut self, bus: &Bus, opcode: u8) -> u32 {
        let target = self.jr_target(bus);
        if self.condition(opcode) {
            self.pc = target;
            12
        } else {
            8
        }
    }

    /// CALL n16 : empile l'adresse de retour, puis saute.
    pub(super) fn call(&mut self, bus: &mut Bus) -> u32 {
        let addr = self.fetch16(bus);
        self.push16(bus, self.pc);
        self.pc = addr;
        24
    }

    /// CALL cc, n16 : 110 cc 100
    pub(super) fn call_cc(&mut self, bus: &mut Bus, opcode: u8) -> u32 {
        let addr = self.fetch16(bus);
        if self.condition(opcode) {
            self.push16(bus, self.pc);
            self.pc = addr;
            24
        } else {
            12
        }
    }

    /// RET : dépile l'adresse de retour.
    pub(super) fn ret(&mut self, bus: &Bus) -> u32 {
        self.pc = self.pop16(bus);
        16
    }

    /// RET cc : 110 cc 000
    pub(super) fn ret_cc(&mut self, bus: &Bus, opcode: u8) -> u32 {
        if self.condition(opcode) {
            self.pc = self.pop16(bus);
            20
        } else {
            8
        }
    }

    /// RST n : 11 ttt 111. Un CALL d'un octet vers l'adresse fixe ttt × 8.
    pub(super) fn rst(&mut self, bus: &mut Bus, opcode: u8) -> u32 {
        self.push16(bus, self.pc);
        self.pc = (opcode & 0x38) as u16;
        16
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::{bus_with, run};
    use super::*;

    #[test]
    fn jp_saute_a_l_adresse() {
        let mut bus = bus_with(&[0xC3, 0x50, 0x01]); // JP 0x0150
        let mut cpu = Cpu::new();
        assert_eq!(run(&mut cpu, &mut bus, 1), 16);
        assert_eq!(cpu.pc, 0x0150);
    }

    #[test]
    fn jp_conditionnel_pris_ou_non() {
        let mut bus = bus_with(&[
            0xAF, //             XOR A          → Z = 1
            0xC2, 0x00, 0x02, // JP NZ, 0x0200  → non pris (12 tics)
            0xCA, 0x00, 0x02, // JP Z, 0x0200   → pris (16 tics)
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 1);
        assert_eq!(run(&mut cpu, &mut bus, 1), 12);
        assert_eq!(cpu.pc, 0x0104); // l'adresse a quand même été lue
        assert_eq!(run(&mut cpu, &mut bus, 1), 16);
        assert_eq!(cpu.pc, 0x0200);
    }

    #[test]
    fn jr_recule() {
        let mut bus = bus_with(&[
            0x00, //       0x0100  NOP
            0x18, 0xFD, // 0x0101  JR -3  → 0x0103 − 3 = 0x0100
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 1);
        assert_eq!(run(&mut cpu, &mut bus, 1), 12);
        assert_eq!(cpu.pc, 0x0100);
    }

    #[test]
    fn jr_conditionnel_non_pris() {
        let mut bus = bus_with(&[
            0xAF, //       XOR A        → Z = 1, C = 0
            0x38, 0x10, // JR C, +16    → non pris (8 tics)
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 1);
        assert_eq!(run(&mut cpu, &mut bus, 1), 8);
        assert_eq!(cpu.pc, 0x0103);
    }

    #[test]
    fn la_boucle_vblank() {
        // La boucle qui attend LY = 144 : elle ne sort que quand la PPU y arrive.
        let mut bus = bus_with(&[
            0xF0, 0x44, // 0x0100  LDH A, (0x44)   lit LY
            0xFE, 0x90, // 0x0102  CP 144
            0x20, 0xFA, // 0x0104  JR NZ, -6       → 0x0100
            0x00, //       0x0106  NOP             atteint seulement quand LY = 144
        ]);
        let mut cpu = Cpu::new();
        while cpu.pc != 0x0106 {
            let cycles = cpu.step(&mut bus).unwrap();
            bus.tick(cycles);
        }
        assert_eq!(bus.read8(0xFF44), 144);
    }

    #[test]
    fn call_puis_ret() {
        let mut program = vec![0; 0x11];
        program[0x00..0x03].copy_from_slice(&[0xCD, 0x10, 0x01]); // 0x0100  CALL 0x0110
        program[0x10] = 0xC9; //                                     0x0110  RET
        let mut bus = bus_with(&program);
        let mut cpu = Cpu::new();

        assert_eq!(run(&mut cpu, &mut bus, 1), 24);
        assert_eq!(cpu.pc, 0x0110);
        assert_eq!(cpu.sp, 0xFFFC);
        assert_eq!(bus.read16(0xFFFC), 0x0103); // le ticket de retour

        assert_eq!(run(&mut cpu, &mut bus, 1), 16);
        assert_eq!(cpu.pc, 0x0103);
        assert_eq!(cpu.sp, 0xFFFE);
    }

    #[test]
    fn ret_conditionnel() {
        let mut bus = bus_with(&[
            0xAF, // XOR A    → Z = 1
            0xC0, // RET NZ   → non pris (8 tics)
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 1);
        assert_eq!(run(&mut cpu, &mut bus, 1), 8);
        assert_eq!(cpu.pc, 0x0102);
        assert_eq!(cpu.sp, 0xFFFE);
    }

    #[test]
    fn rst_appelle_une_adresse_fixe() {
        let mut bus = bus_with(&[0xEF]); // RST 0x28
        let mut cpu = Cpu::new();
        assert_eq!(run(&mut cpu, &mut bus, 1), 16);
        assert_eq!(cpu.pc, 0x0028);
        assert_eq!(bus.read16(cpu.sp), 0x0101);
    }
}
