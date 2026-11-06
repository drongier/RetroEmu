//! CPU SM83 : les registres et la boucle « lire → décoder → exécuter ».
//!
//! Pour l'instant seul NOP est décodé. Les ~500 instructions arrivent à l'étape 3.

use anyhow::{Result, bail};

use crate::bus::Bus;

pub struct Cpu {
    a: u8,
    /// Drapeaux : Z (bit 7), N (bit 6), H (bit 5), C (bit 4). Les bits 0-3 valent toujours 0.
    f: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    h: u8,
    l: u8,
    /// Stack Pointer : adresse du dessus de la pile.
    sp: u16,
    /// Program Counter : adresse de la prochaine instruction.
    pc: u16,
}

impl Cpu {
    /// Registres tels que la boot ROM DMG les laisse en sautant au jeu (0x0100).
    pub fn new() -> Cpu {
        Cpu {
            a: 0x01,
            f: 0xB0,
            b: 0x00,
            c: 0x13,
            d: 0x00,
            e: 0xD8,
            h: 0x01,
            l: 0x4D,
            sp: 0xFFFE,
            pc: 0x0100,
        }
    }

    /// Exécute une instruction. Renvoie le nombre de T-cycles qu'elle a pris.
    pub fn step(&mut self, bus: &mut Bus) -> Result<u32> {
        let addr = self.pc;
        let opcode = self.fetch8(bus);
        match opcode {
            0x00 => Ok(4), // NOP : ne fait rien
            _ => bail!("opcode {opcode:#04X} pas encore supporté (adresse {addr:#06X})"),
        }
    }

    /// Lit l'octet pointé par PC, puis avance PC.
    fn fetch8(&mut self, bus: &Bus) -> u8 {
        let byte = bus.read8(self.pc);
        self.pc = self.pc.wrapping_add(1);
        byte
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::Cartridge;

    /// Bus avec une ROM vide, et `program` placé à l'entry point 0x0100.
    fn bus_with(program: &[u8]) -> Bus {
        let mut rom = vec![0; 0x8000];
        rom[0x0100..0x0100 + program.len()].copy_from_slice(program);
        Bus::new(Cartridge::from_bytes(rom).unwrap())
    }

    #[test]
    fn demarre_a_l_entry_point() {
        let cpu = Cpu::new();
        assert_eq!(cpu.pc, 0x0100);
        assert_eq!(cpu.sp, 0xFFFE);
    }

    #[test]
    fn nop_avance_pc_et_prend_4_cycles() {
        let mut bus = bus_with(&[0x00, 0x00]);
        let mut cpu = Cpu::new();
        assert_eq!(cpu.step(&mut bus).unwrap(), 4);
        assert_eq!(cpu.pc, 0x0101);
        assert_eq!(cpu.step(&mut bus).unwrap(), 4);
        assert_eq!(cpu.pc, 0x0102);
    }

    #[test]
    fn opcode_inconnu_donne_une_erreur_claire() {
        let mut bus = bus_with(&[0x00, 0xC3]);
        let mut cpu = Cpu::new();
        cpu.step(&mut bus).unwrap();
        let err = cpu.step(&mut bus).unwrap_err();
        assert_eq!(err.to_string(), "opcode 0xC3 pas encore supporté (adresse 0x0101)");
    }
}
