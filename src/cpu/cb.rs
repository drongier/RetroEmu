//! Les instructions préfixées par 0xCB : 256 opcodes de plus, lus dans l'octet suivant.
//!
//! Le second octet se découpe en xx yyy zzz :
//! - zzz : le registre (0 B, 1 C, 2 D, 3 E, 4 H, 5 L, 6 (HL), 7 A) ;
//! - xx  : la famille (0 rotation/décalage, 1 BIT, 2 RES, 3 SET) ;
//! - yyy : le numéro du bit, ou la rotation quand xx = 0
//!   (0 RLC, 1 RRC, 2 RL, 3 RR, 4 SLA, 5 SRA, 6 SWAP, 7 SRL).

use super::{Cpu, FLAG_C, FLAG_H, FLAG_N, FLAG_Z};
use crate::bus::Bus;

impl Cpu {
    /// Exécute l'instruction qui suit le préfixe 0xCB (déjà lu par `step`).
    pub(super) fn cb(&mut self, bus: &mut Bus) -> u32 {
        let opcode = self.fetch8(bus);
        let y = (opcode >> 3) & 0x07;
        let code = opcode & 0x07;
        let value = self.read_r8(bus, code);

        match opcode >> 6 {
            0 => {
                let result = self.rotate(y, value);
                self.write_r8(bus, code, result);
            }
            1 => {
                // BIT : on lit sans rien réécrire, d'où un coût plus faible sur (HL).
                self.set_flag(FLAG_Z, value & (1 << y) == 0);
                self.set_flag(FLAG_N, false);
                self.set_flag(FLAG_H, true);
                return if code == 6 { 12 } else { 8 };
            }
            2 => self.write_r8(bus, code, value & !(1 << y)), // RES : aucun drapeau
            3 => self.write_r8(bus, code, value | (1 << y)),  // SET : aucun drapeau
            _ => unreachable!(),
        }
        if code == 6 { 16 } else { 8 }
    }

    /// Applique la rotation ou le décalage `op` à `value`.
    /// Z selon le résultat, N = 0, H = 0, C = le bit sorti.
    /// Aussi utilisée par RLCA, RRCA, RLA, RRA (misc.rs).
    pub(super) fn rotate(&mut self, op: u8, value: u8) -> u8 {
        let carry = self.flag(FLAG_C) as u8;
        let bit7 = value & 0x80 != 0;
        let bit0 = value & 0x01 != 0;

        let (result, carry_out) = match op {
            0 => (value.rotate_left(1), bit7),             // RLC
            1 => (value.rotate_right(1), bit0),            // RRC
            2 => ((value << 1) | carry, bit7),             // RL : l'ancien C entre à droite
            3 => ((value >> 1) | (carry << 7), bit0),      // RR : l'ancien C entre à gauche
            4 => (value << 1, bit7),                       // SLA
            5 => ((value >> 1) | (value & 0x80), bit0),    // SRA : le bit 7 se recopie
            6 => (value.rotate_left(4), false),            // SWAP : échange les moitiés
            7 => (value >> 1, bit0),                       // SRL
            _ => unreachable!(),
        };

        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_H, false);
        self.set_flag(FLAG_C, carry_out);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::{bus_with, run};
    use super::*;

    /// Charge `value` dans B, puis exécute l'instruction 0xCB `opcode` sur B.
    fn cb_on_b(value: u8, opcode: u8, f: u8) -> Cpu {
        let mut bus = bus_with(&[0x06, value, 0xCB, opcode]); // LD B, value ; CB opcode
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 1);
        cpu.f = f;
        assert_eq!(run(&mut cpu, &mut bus, 1), 8);
        cpu
    }

    #[test]
    fn swap_echange_les_moities() {
        let cpu = cb_on_b(0xA3, 0x30, FLAG_C); // SWAP B
        assert_eq!(cpu.b, 0x3A);
        assert_eq!(cpu.f, 0); // C remis à 0
    }

    #[test]
    fn bit_met_z_quand_le_bit_vaut_0() {
        let cpu = cb_on_b(0b0111_1111, 0x78, FLAG_C); // BIT 7, B
        assert_eq!(cpu.f, FLAG_Z | FLAG_H | FLAG_C); // Z = 1, H = 1, C inchangé

        let cpu = cb_on_b(0b1000_0000, 0x78, 0); // BIT 7, B
        assert_eq!(cpu.f, FLAG_H); // le bit vaut 1 → Z = 0
        assert_eq!(cpu.b, 0b1000_0000); // BIT ne modifie pas le registre
    }

    #[test]
    fn res_et_set_ne_touchent_pas_les_drapeaux() {
        let cpu = cb_on_b(0xFF, 0xB8, 0xF0); // RES 7, B
        assert_eq!(cpu.b, 0x7F);
        assert_eq!(cpu.f, 0xF0);

        let cpu = cb_on_b(0x00, 0xC0, 0x00); // SET 0, B
        assert_eq!(cpu.b, 0x01);
        assert_eq!(cpu.f, 0x00);
    }

    #[test]
    fn rl_passe_par_la_retenue() {
        let cpu = cb_on_b(0b1000_0001, 0x10, FLAG_C); // RL B, avec C = 1
        assert_eq!(cpu.b, 0b0000_0011); // l'ancien C entre à droite
        assert_eq!(cpu.f, FLAG_C); // le bit 7 sort dans C
    }

    #[test]
    fn rlc_fait_le_tour() {
        let cpu = cb_on_b(0b1000_0001, 0x00, 0); // RLC B
        assert_eq!(cpu.b, 0b0000_0011);
        assert_eq!(cpu.f, FLAG_C);
    }

    #[test]
    fn sra_garde_le_signe_et_srl_non() {
        let cpu = cb_on_b(0xF0, 0x28, 0); // SRA B : -16 / 2 = -8
        assert_eq!(cpu.b, 0xF8);

        let cpu = cb_on_b(0xF0, 0x38, 0); // SRL B : 240 / 2 = 120
        assert_eq!(cpu.b, 0x78);
    }

    #[test]
    fn sla_qui_tombe_a_zero_leve_z() {
        let cpu = cb_on_b(0x80, 0x20, 0); // SLA B
        assert_eq!(cpu.b, 0x00);
        assert_eq!(cpu.f, FLAG_Z | FLAG_C);
    }

    #[test]
    fn sur_hl_les_cycles_changent() {
        let mut bus = bus_with(&[
            0x21, 0x00, 0xC0, // LD HL, 0xC000
            0xCB, 0xFE, //       SET 7, (HL)   (16 tics : lire + écrire)
            0xCB, 0x7E, //       BIT 7, (HL)   (12 tics : lire seulement)
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 1);
        assert_eq!(run(&mut cpu, &mut bus, 1), 16);
        assert_eq!(bus.read8(0xC000), 0x80);
        assert_eq!(run(&mut cpu, &mut bus, 1), 12);
        assert!(!cpu.flag(FLAG_Z));
    }
}
