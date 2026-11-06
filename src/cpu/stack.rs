//! La pile : PUSH, POP, et les outils push16/pop16 que CALL, RET, RST
//! (et plus tard les interruptions) utilisent aussi.
//!
//! La pile descend : empiler fait reculer SP. SP pointe sur la dernière valeur posée.

use super::Cpu;
use crate::bus::Bus;

impl Cpu {
    /// Empile une valeur 16 bits : l'octet fort d'abord, l'octet faible finit à l'adresse la plus basse.
    pub(super) fn push16(&mut self, bus: &mut Bus, value: u16) {
        self.sp = self.sp.wrapping_sub(1);
        bus.write8(self.sp, (value >> 8) as u8);
        self.sp = self.sp.wrapping_sub(1);
        bus.write8(self.sp, value as u8);
    }

    /// Dépile une valeur 16 bits.
    pub(super) fn pop16(&mut self, bus: &Bus) -> u16 {
        let low = bus.read8(self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        let high = bus.read8(self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        (high << 8) | low
    }

    /// PUSH rr : 11 pp0 101. Ici le code 3 désigne AF, pas SP.
    pub(super) fn push_rr(&mut self, bus: &mut Bus, opcode: u8) -> u32 {
        let value = match (opcode >> 4) & 0x03 {
            3 => self.af(),
            code => self.read_r16(code),
        };
        self.push16(bus, value);
        16
    }

    /// POP rr : 11 pp0 001. Ici le code 3 désigne AF, pas SP.
    pub(super) fn pop_rr(&mut self, bus: &Bus, opcode: u8) -> u32 {
        let value = self.pop16(bus);
        match (opcode >> 4) & 0x03 {
            3 => self.set_af(value), // set_af efface les 4 bits bas de F
            code => self.write_r16(code, value),
        }
        12
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::{bus_with, run};
    use super::*;

    #[test]
    fn push_puis_pop_dans_une_autre_paire() {
        let mut bus = bus_with(&[
            0x01, 0x34, 0x12, // LD BC, 0x1234
            0xC5, //             PUSH BC   (16 tics)
            0xD1, //             POP DE    (12 tics)
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 1);
        assert_eq!(run(&mut cpu, &mut bus, 1), 16);
        assert_eq!(cpu.sp, 0xFFFC);
        assert_eq!(bus.read8(0xFFFD), 0x12); // octet fort en haut
        assert_eq!(bus.read8(0xFFFC), 0x34); // octet faible en bas

        assert_eq!(run(&mut cpu, &mut bus, 1), 12);
        assert_eq!(cpu.de(), 0x1234);
        assert_eq!(cpu.sp, 0xFFFE);
    }

    #[test]
    fn pop_af_efface_les_bits_bas_de_f() {
        let mut bus = bus_with(&[
            0x01, 0xFF, 0xFF, // LD BC, 0xFFFF
            0xC5, //             PUSH BC
            0xF1, //             POP AF
        ]);
        let mut cpu = Cpu::new();
        run(&mut cpu, &mut bus, 3);
        assert_eq!(cpu.a, 0xFF);
        assert_eq!(cpu.f, 0xF0);
    }
}
