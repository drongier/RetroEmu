//! Les LD : copier une valeur d'un endroit à un autre. Aucune ne touche aux drapeaux.
//!
//! Chaque fonction exécute une instruction dont l'opcode est déjà lu, et renvoie ses T-cycles.

use super::Cpu;
use crate::bus::Bus;

impl Cpu {
    /// LD r, r' : 01 ddd sss
    pub(super) fn ld_r_r(&mut self, bus: &mut Bus, opcode: u8) -> u32 {
        let dst = (opcode >> 3) & 0x07;
        let src = opcode & 0x07;
        let value = self.read_r8(bus, src);
        self.write_r8(bus, dst, value);
        if dst == 6 || src == 6 { 8 } else { 4 }
    }

    /// LD r, n8 : 00 ddd 110
    pub(super) fn ld_r_n8(&mut self, bus: &mut Bus, opcode: u8) -> u32 {
        let dst = (opcode >> 3) & 0x07;
        let value = self.fetch8(bus);
        self.write_r8(bus, dst, value);
        if dst == 6 { 12 } else { 8 }
    }

    /// LD rr, n16 : 00 pp0 001
    pub(super) fn ld_rr_n16(&mut self, bus: &mut Bus, opcode: u8) -> u32 {
        let value = self.fetch16(bus);
        self.write_r16(opcode >> 4, value);
        12
    }

    /// LD (BC), A · LD (DE), A · LD (HL+), A · LD (HL-), A : 00 pp0 010
    pub(super) fn ld_indirect_a(&mut self, bus: &mut Bus, opcode: u8) -> u32 {
        let addr = self.indirect_addr(opcode >> 4);
        bus.write8(addr, self.a);
        8
    }

    /// LD A, (BC) · LD A, (DE) · LD A, (HL+) · LD A, (HL-) : 00 pp1 010
    pub(super) fn ld_a_indirect(&mut self, bus: &mut Bus, opcode: u8) -> u32 {
        let addr = self.indirect_addr(opcode >> 4);
        self.a = bus.read8(addr);
        8
    }

    /// LDH (n8), A : raccourci vers 0xFF00 + n8 (registres I/O, HRAM)
    pub(super) fn ldh_n8_a(&mut self, bus: &mut Bus) -> u32 {
        let addr = 0xFF00 | self.fetch8(bus) as u16;
        bus.write8(addr, self.a);
        12
    }

    /// LDH A, (n8)
    pub(super) fn ldh_a_n8(&mut self, bus: &mut Bus) -> u32 {
        let addr = 0xFF00 | self.fetch8(bus) as u16;
        self.a = bus.read8(addr);
        12
    }

    /// LD (C), A : comme LDH, mais l'octet bas de l'adresse est dans C
    pub(super) fn ld_c_a(&mut self, bus: &mut Bus) -> u32 {
        bus.write8(0xFF00 | self.c as u16, self.a);
        8
    }

    /// LD A, (C)
    pub(super) fn ld_a_c(&mut self, bus: &mut Bus) -> u32 {
        self.a = bus.read8(0xFF00 | self.c as u16);
        8
    }

    /// LD (n16), A : n'importe quelle adresse
    pub(super) fn ld_n16_a(&mut self, bus: &mut Bus) -> u32 {
        let addr = self.fetch16(bus);
        bus.write8(addr, self.a);
        16
    }

    /// LD A, (n16)
    pub(super) fn ld_a_n16(&mut self, bus: &mut Bus) -> u32 {
        let addr = self.fetch16(bus);
        self.a = bus.read8(addr);
        16
    }

    /// LD (n16), SP : écrit SP sur 2 octets
    pub(super) fn ld_n16_sp(&mut self, bus: &mut Bus) -> u32 {
        let addr = self.fetch16(bus);
        bus.write16(addr, self.sp);
        20
    }

    /// LD SP, HL
    pub(super) fn ld_sp_hl(&mut self) -> u32 {
        self.sp = self.hl();
        8
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::{bus_with, run};
    use super::*;

    #[test]
    fn ld_registre_valeur_puis_registre_registre() {
        let mut bus = bus_with(&[
            0x3E, 0x05, // LD A, 0x05
            0x47, //       LD B, A
            0x48, //       LD C, B
        ]);
        let mut cpu = Cpu::new();
        assert_eq!(run(&mut cpu, &mut bus, 3), 8 + 4 + 4);
        assert_eq!((cpu.a, cpu.b, cpu.c), (0x05, 0x05, 0x05));
        assert_eq!(cpu.f, 0xB0); // les LD ne touchent jamais aux drapeaux
    }

    #[test]
    fn ld_via_hl_lit_et_ecrit_la_memoire() {
        let mut bus = bus_with(&[
            0x21, 0x00, 0xC0, // LD HL, 0xC000
            0x36, 0x42, //       LD (HL), 0x42
            0x7E, //             LD A, (HL)
        ]);
        let mut cpu = Cpu::new();
        assert_eq!(run(&mut cpu, &mut bus, 3), 12 + 12 + 8);
        assert_eq!(bus.read8(0xC000), 0x42);
        assert_eq!(cpu.a, 0x42);
    }

    #[test]
    fn hl_plus_et_hl_moins_deplacent_hl() {
        let mut bus = bus_with(&[
            0x21, 0x00, 0xC0, // LD HL, 0xC000
            0x3E, 0x11, //       LD A, 0x11
            0x22, //             LD (HL+), A   → écrit en C000, HL = C001
            0x32, //             LD (HL-), A   → écrit en C001, HL = C000
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 4);
        assert_eq!(bus.read8(0xC000), 0x11);
        assert_eq!(bus.read8(0xC001), 0x11);
        assert_eq!(cpu.hl(), 0xC000);
    }

    #[test]
    fn ldh_parle_aux_registres_io() {
        let mut bus = bus_with(&[
            0x3E, 0x04, // LD A, 0x04
            0xE0, 0x07, // LDH (0x07), A  → TAC = 0x04
            0x3E, 0x00, // LD A, 0x00
            0xF0, 0x07, // LDH A, (0x07)  → relit TAC
        ]);
        let mut cpu = Cpu::new();
        assert_eq!(run(&mut cpu, &mut bus, 4), 8 + 12 + 8 + 12);
        assert_eq!(cpu.a, 0xFC); // 0x04 + les 5 bits inexistants de TAC lus à 1
    }

    #[test]
    fn ld_adresse_absolue_et_sp() {
        let mut bus = bus_with(&[
            0x3E, 0x99, //       LD A, 0x99
            0xEA, 0x00, 0xC1, // LD (0xC100), A
            0x3E, 0x00, //       LD A, 0x00
            0xFA, 0x00, 0xC1, // LD A, (0xC100)
            0x31, 0x34, 0x12, // LD SP, 0x1234
            0x08, 0x00, 0xC0, // LD (0xC000), SP
        ]);
        let mut cpu = Cpu::new();
        assert_eq!(run(&mut cpu, &mut bus, 6), 8 + 16 + 8 + 16 + 12 + 20);
        assert_eq!(cpu.a, 0x99);
        assert_eq!(bus.read8(0xC000), 0x34); // little-endian : octet faible d'abord
        assert_eq!(bus.read8(0xC001), 0x12);
    }
}
