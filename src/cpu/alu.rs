//! L'ALU 8 bits : A (op) valeur → A, et les drapeaux dans F. Plus INC et DEC sur 8 bits.
//!
//! Opérations selon leur code ooo : 0 ADD, 1 ADC, 2 SUB, 3 SBC, 4 AND, 5 XOR, 6 OR, 7 CP.

use super::{Cpu, FLAG_C, FLAG_H, FLAG_N, FLAG_Z};
use crate::bus::Bus;

impl Cpu {
    /// ALU A, r : 10 ooo sss
    pub(super) fn alu_r(&mut self, bus: &mut Bus, opcode: u8) -> u32 {
        let src = opcode & 0x07;
        let value = self.read_r8(bus, src);
        self.alu((opcode >> 3) & 0x07, value);
        if src == 6 { 8 } else { 4 }
    }

    /// ALU A, n8 : 11 ooo 110
    pub(super) fn alu_n8(&mut self, bus: &mut Bus, opcode: u8) -> u32 {
        let value = self.fetch8(bus);
        self.alu((opcode >> 3) & 0x07, value);
        8
    }

    /// INC r : 00 rrr 100. Ne touche pas à C.
    pub(super) fn inc_r(&mut self, bus: &mut Bus, opcode: u8) -> u32 {
        let code = (opcode >> 3) & 0x07;
        let value = self.read_r8(bus, code);
        let result = value.wrapping_add(1);
        self.write_r8(bus, code, result);
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_H, value & 0x0F == 0x0F); // 1111 + 1 : retenue vers le bit 4
        if code == 6 { 12 } else { 4 }
    }

    /// DEC r : 00 rrr 101. Ne touche pas à C.
    pub(super) fn dec_r(&mut self, bus: &mut Bus, opcode: u8) -> u32 {
        let code = (opcode >> 3) & 0x07;
        let value = self.read_r8(bus, code);
        let result = value.wrapping_sub(1);
        self.write_r8(bus, code, result);
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_N, true);
        self.set_flag(FLAG_H, value & 0x0F == 0x00); // 0000 − 1 : emprunt au bit 4
        if code == 6 { 12 } else { 4 }
    }

    /// Effectue l'opération `op` entre A et `value`, et met à jour les 4 drapeaux.
    fn alu(&mut self, op: u8, value: u8) {
        let a = self.a;
        let carry = self.flag(FLAG_C) as u8;

        let (result, half_carry, carry_out) = match op {
            // ADD, ADC
            0 | 1 => {
                let carry = if op == 1 { carry } else { 0 };
                let sum = a as u16 + value as u16 + carry as u16;
                let half = (a & 0x0F) + (value & 0x0F) + carry > 0x0F;
                (sum as u8, half, sum > 0xFF)
            }
            // SUB, SBC, CP : CP est une soustraction dont on jette le résultat
            2 | 3 | 7 => {
                let carry = if op == 3 { carry } else { 0 };
                let diff = a as i16 - value as i16 - carry as i16;
                let half = (a & 0x0F) < (value & 0x0F) + carry;
                (diff as u8, half, diff < 0)
            }
            4 => (a & value, true, false), // AND met H à 1 : bizarrerie du circuit
            5 => (a ^ value, false, false),
            6 => (a | value, false, false),
            _ => unreachable!("code d'opération ALU invalide : {op}"),
        };

        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_N, matches!(op, 2 | 3 | 7));
        self.set_flag(FLAG_H, half_carry);
        self.set_flag(FLAG_C, carry_out);
        if op != 7 {
            self.a = result;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::{bus_with, run};
    use super::*;

    /// Exécute `LD A, a` puis l'instruction ALU `program`, et renvoie le CPU.
    fn alu_with(a: u8, program: &[u8]) -> Cpu {
        let mut code = vec![0x3E, a];
        code.extend_from_slice(program);
        let mut bus = bus_with(&code);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 2);
        cpu
    }

    #[test]
    fn add_simple() {
        let cpu = alu_with(0x05, &[0xC6, 0x03]); // ADD A, 0x03
        assert_eq!(cpu.a, 0x08);
        assert_eq!(cpu.f, 0x00); // ni zéro, ni soustraction, ni retenue
    }

    #[test]
    fn add_half_carry() {
        let cpu = alu_with(0x0F, &[0xC6, 0x01]); // 0x0F + 0x01
        assert_eq!(cpu.a, 0x10);
        assert_eq!(cpu.f, FLAG_H); // la moitié basse a débordé, pas l'octet entier
    }

    #[test]
    fn add_deborde_et_donne_zero() {
        let cpu = alu_with(0xFF, &[0xC6, 0x01]); // 0xFF + 0x01
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.f, FLAG_Z | FLAG_H | FLAG_C);
    }

    #[test]
    fn adc_ajoute_la_retenue() {
        // ADD A, 0xFF lève C, puis ADC A, 0x00 ajoute cette retenue.
        let mut bus = bus_with(&[0x3E, 0x01, 0xC6, 0xFF, 0xCE, 0x00]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 3);
        assert_eq!(cpu.a, 0x01); // 0x01 + 0xFF = 0x00 (C=1), puis 0x00 + 0x00 + 1
        assert!(!cpu.flag(FLAG_C));
    }

    #[test]
    fn sub_emprunte() {
        let cpu = alu_with(0x10, &[0xD6, 0x01]); // 0x10 - 0x01
        assert_eq!(cpu.a, 0x0F);
        assert_eq!(cpu.f, FLAG_N | FLAG_H); // emprunt dans la moitié basse

        let cpu = alu_with(0x03, &[0xD6, 0x05]); // 3 - 5
        assert_eq!(cpu.a, 0xFE);
        assert_eq!(cpu.f, FLAG_N | FLAG_H | FLAG_C);
    }

    #[test]
    fn cp_compare_sans_modifier_a() {
        let cpu = alu_with(144, &[0xFE, 144]); // CP 144
        assert_eq!(cpu.a, 144);
        assert!(cpu.flag(FLAG_Z)); // égal

        let cpu = alu_with(100, &[0xFE, 144]);
        assert!(cpu.flag(FLAG_C)); // A plus petit
        assert!(!cpu.flag(FLAG_Z));
    }

    #[test]
    fn xor_a_met_a_zero() {
        let cpu = alu_with(0x42, &[0xAF]); // XOR A, A
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.f, FLAG_Z);
    }

    #[test]
    fn and_leve_toujours_h() {
        let cpu = alu_with(0xF0, &[0xE6, 0x0F]); // AND 0x0F
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.f, FLAG_Z | FLAG_H);
    }

    #[test]
    fn alu_avec_un_registre_et_avec_hl() {
        let mut bus = bus_with(&[
            0x06, 0x02, //       LD B, 0x02
            0x21, 0x00, 0xC0, // LD HL, 0xC000
            0x36, 0x10, //       LD (HL), 0x10
            0x3E, 0x01, //       LD A, 0x01
            0x80, //             ADD A, B     (4 tics)
            0xB6, //             OR A, (HL)   (8 tics)
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 4);
        assert_eq!(run(&mut cpu, &mut bus, 2), 4 + 8);
        assert_eq!(cpu.a, 0x13); // (1 + 2) | 0x10
    }

    #[test]
    fn inc_leve_h_et_garde_c() {
        let mut bus = bus_with(&[
            0x06, 0x0F, // LD B, 0x0F
            0x04, //       INC B
        ]);
        let mut cpu = Cpu::new(); // F = 0xB0 au départ : C est levé
        run(&mut cpu, &mut bus, 2);
        assert_eq!(cpu.b, 0x10);
        assert_eq!(cpu.f, FLAG_H | FLAG_C); // Z et N baissés, H levé, C intact
    }

    #[test]
    fn inc_de_0xff_donne_zero() {
        let mut bus = bus_with(&[0x0E, 0xFF, 0x0C]); // LD C, 0xFF ; INC C
        let mut cpu = Cpu::new();
        cpu.f = 0;
        run(&mut cpu, &mut bus, 2);
        assert_eq!(cpu.c, 0x00);
        assert_eq!(cpu.f, FLAG_Z | FLAG_H); // C reste à 0 : INC ne le lève jamais
    }

    #[test]
    fn dec_leve_n_et_z() {
        let mut bus = bus_with(&[0x3E, 0x01, 0x3D]); // LD A, 0x01 ; DEC A
        let mut cpu = Cpu::new();
        cpu.f = 0;
        run(&mut cpu, &mut bus, 2);
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.f, FLAG_Z | FLAG_N);

        let mut bus = bus_with(&[0x16, 0x10, 0x15]); // LD D, 0x10 ; DEC D
        let mut cpu = Cpu::new();
        cpu.f = 0;
        run(&mut cpu, &mut bus, 2);
        assert_eq!(cpu.d, 0x0F);
        assert_eq!(cpu.f, FLAG_N | FLAG_H); // emprunt dans la moitié basse
    }

    #[test]
    fn inc_hl_modifie_la_memoire() {
        let mut bus = bus_with(&[
            0x21, 0x00, 0xC0, // LD HL, 0xC000
            0x36, 0x41, //       LD (HL), 0x41
            0x34, //             INC (HL)   (12 tics)
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 2);
        assert_eq!(run(&mut cpu, &mut bus, 1), 12);
        assert_eq!(bus.read8(0xC000), 0x42);
    }
}
