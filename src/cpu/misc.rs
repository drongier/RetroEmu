//! Les petites instructions : CPL, SCF, CCF, DAA, et les rotations express de A.

use super::{Cpu, FLAG_C, FLAG_H, FLAG_N, FLAG_Z};

impl Cpu {
    /// CPL : inverse tous les bits de A.
    pub(super) fn cpl(&mut self) -> u32 {
        self.a = !self.a;
        self.set_flag(FLAG_N, true);
        self.set_flag(FLAG_H, true);
        4
    }

    /// SCF : met C à 1.
    pub(super) fn scf(&mut self) -> u32 {
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_H, false);
        self.set_flag(FLAG_C, true);
        4
    }

    /// CCF : inverse C.
    pub(super) fn ccf(&mut self) -> u32 {
        let carry = self.flag(FLAG_C);
        self.set_flag(FLAG_N, false);
        self.set_flag(FLAG_H, false);
        self.set_flag(FLAG_C, !carry);
        4
    }

    /// RLCA, RRCA, RLA, RRA : 000 ooo 111. Les rotations CB 0 à 3 sur A, mais Z vaut toujours 0.
    pub(super) fn rotate_a(&mut self, opcode: u8) -> u32 {
        self.a = self.rotate(opcode >> 3, self.a);
        self.set_flag(FLAG_Z, false);
        4
    }

    /// DAA : corrige A en BCD (un chiffre décimal par moitié) après une addition ou une soustraction.
    /// N dit dans quel sens corriger, H et C disent quelles moitiés ont débordé.
    pub(super) fn daa(&mut self) -> u32 {
        let mut a = self.a;
        let mut carry = self.flag(FLAG_C);
        let half = self.flag(FLAG_H);

        if !self.flag(FLAG_N) {
            // Après une addition : on saute les 6 faux chiffres A à F.
            if carry || a > 0x99 {
                a = a.wrapping_add(0x60);
                carry = true;
            }
            if half || a & 0x0F > 0x09 {
                a = a.wrapping_add(0x06);
            }
        } else {
            // Après une soustraction : H et C suffisent à savoir quoi retirer.
            if carry {
                a = a.wrapping_sub(0x60);
            }
            if half {
                a = a.wrapping_sub(0x06);
            }
        }

        self.a = a;
        self.set_flag(FLAG_Z, a == 0);
        self.set_flag(FLAG_H, false);
        self.set_flag(FLAG_C, carry);
        4
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::{bus_with, run};
    use super::*;

    /// Exécute `program` en entier et renvoie le CPU.
    fn exec(program: &[u8], count: usize) -> Cpu {
        let mut bus = bus_with(program);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, count);
        cpu
    }

    #[test]
    fn cpl_retourne_le_joypad() {
        let cpu = exec(&[0x3E, 0b1110_1101, 0x2F], 2); // LD A, P1 simulé ; CPL
        assert_eq!(cpu.a, 0b0001_0010);
        assert!(cpu.flag(FLAG_N) && cpu.flag(FLAG_H));
    }

    #[test]
    fn scf_puis_ccf_met_c_a_zero() {
        let cpu = exec(&[0x37], 1); // SCF
        assert!(cpu.flag(FLAG_C));
        let cpu = exec(&[0x37, 0x3F], 2); // SCF ; CCF
        assert!(!cpu.flag(FLAG_C));
        assert!(cpu.flag(FLAG_Z)); // Z ne bouge pas (il valait 1 après le boot)
    }

    #[test]
    fn rlca_ne_leve_jamais_z() {
        let cpu = exec(&[0xAF, 0x07], 2); // XOR A ; RLCA → résultat 0
        assert_eq!(cpu.a, 0);
        assert!(!cpu.flag(FLAG_Z)); // RLC A aurait levé Z
    }

    #[test]
    fn rla_passe_par_la_retenue() {
        let cpu = exec(&[0x3E, 0x80, 0x37, 0x17], 3); // LD A, 0x80 ; SCF ; RLA
        assert_eq!(cpu.a, 0x01); // l'ancien C est entré à droite
        assert!(cpu.flag(FLAG_C)); // le bit 7 est sorti dans C
    }

    #[test]
    fn daa_19_plus_1_donne_20() {
        let cpu = exec(&[0x3E, 0x19, 0xC6, 0x01, 0x27], 3); // LD A, 0x19 ; ADD A, 1 ; DAA
        assert_eq!(cpu.a, 0x20);
        assert!(!cpu.flag(FLAG_C));
    }

    #[test]
    fn daa_a_besoin_de_h() {
        let cpu = exec(&[0x3E, 0x09, 0xC6, 0x09, 0x27], 3); // 0x09 + 0x09 = 0x12, H = 1
        assert_eq!(cpu.a, 0x18); // 9 + 9 = 18
    }

    #[test]
    fn daa_deborde_dans_c() {
        let cpu = exec(&[0x3E, 0x90, 0xC6, 0x90, 0x27], 3); // 90 + 90
        assert_eq!(cpu.a, 0x80); // 180 : on garde « 80 »…
        assert!(cpu.flag(FLAG_C)); // … et la centaine part dans C
    }

    #[test]
    fn daa_apres_une_soustraction() {
        let cpu = exec(&[0x3E, 0x20, 0xD6, 0x01, 0x27], 3); // 20 − 1
        assert_eq!(cpu.a, 0x19);
        assert!(cpu.flag(FLAG_N)); // N inchangé
    }
}
