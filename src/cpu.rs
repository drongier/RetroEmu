//! CPU SM83 : les registres et la boucle « lire → décoder → exécuter ».
//!
//! Ce fichier contient la table des opcodes (`step`). Chaque famille d'instructions
//! est rangée dans son propre fichier du dossier `cpu/`.
//!
//! Les 245 opcodes du SM83 et les 256 instructions CB sont tous décodés. Le `match` de `step`
//! n'a pas de cas `_` : le compilateur vérifie que les 256 valeurs d'un octet sont couvertes.

mod alu;
mod arith16;
mod cb;
mod interrupts;
mod jumps;
mod load;
mod misc;
mod registers;
mod stack;
#[cfg(test)]
mod test_utils;

use anyhow::{Result, bail};

use crate::bus::Bus;

/// Bits du registre F.
const FLAG_Z: u8 = 0x80; // le résultat vaut zéro
const FLAG_N: u8 = 0x40; // la dernière opération était une soustraction
const FLAG_H: u8 = 0x20; // retenue du bit 3 vers le bit 4 (half-carry)
const FLAG_C: u8 = 0x10; // retenue du bit 7 (carry)

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
    /// Interrupt Master Enable : l'interrupteur général des interruptions (DI / EI).
    ime: bool,
    /// EI a été exécuté : IME passera à 1 après l'instruction suivante.
    ei_pending: bool,
    /// Le CPU dort (HALT) jusqu'à ce qu'une interruption soit en attente.
    halted: bool,
    /// HALT bug : le prochain octet lu ne fera pas avancer PC.
    halt_bug: bool,
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
            ime: false,
            ei_pending: false,
            halted: false,
            halt_bug: false,
        }
    }

    /// Exécute une instruction, ou sert une interruption. Renvoie le nombre de T-cycles pris.
    pub fn step(&mut self, bus: &mut Bus) -> Result<u32> {
        if self.halted {
            if bus.pending_interrupts() == 0 {
                return Ok(4); // il dort, mais le temps passe pour les autres puces
            }
            self.halted = false;
        }
        if self.ime && bus.pending_interrupts() != 0 {
            return Ok(self.service_interrupt(bus));
        }
        // Le retard d'EI : IME s'allume après le test ci-dessus, donc juste avant
        // l'instruction qui suit EI. Une interruption ne pourra couper qu'après elle.
        if self.ei_pending {
            self.ei_pending = false;
            self.ime = true;
        }

        let addr = self.pc;
        let opcode = self.fetch8(bus);
        let cycles = match opcode {
            0x00 => 4, // NOP : ne fait rien

            // --- LD (load.rs) ---
            // 0x76 serait « LD (HL), (HL) » : c'est HALT. Son bras doit passer avant la plage
            // des LD, car le premier bras qui correspond gagne.
            0x76 => self.halt(bus),
            0x40..=0x7F => self.ld_r_r(bus, opcode),
            0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => self.ld_r_n8(bus, opcode),
            0x01 | 0x11 | 0x21 | 0x31 => self.ld_rr_n16(bus, opcode),
            0x02 | 0x12 | 0x22 | 0x32 => self.ld_indirect_a(bus, opcode),
            0x0A | 0x1A | 0x2A | 0x3A => self.ld_a_indirect(bus, opcode),
            0xE0 => self.ldh_n8_a(bus),
            0xF0 => self.ldh_a_n8(bus),
            0xE2 => self.ld_c_a(bus),
            0xF2 => self.ld_a_c(bus),
            0xEA => self.ld_n16_a(bus),
            0xFA => self.ld_a_n16(bus),
            0x08 => self.ld_n16_sp(bus),
            0xF9 => self.ld_sp_hl(),

            // --- ALU 8 bits (alu.rs) ---
            0x80..=0xBF => self.alu_r(bus, opcode),
            0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => self.alu_n8(bus, opcode),
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C => self.inc_r(bus, opcode),
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D => self.dec_r(bus, opcode),

            // --- Calculs 16 bits (arith16.rs) ---
            0x03 | 0x13 | 0x23 | 0x33 => self.inc_rr(opcode),
            0x0B | 0x1B | 0x2B | 0x3B => self.dec_rr(opcode),
            0x09 | 0x19 | 0x29 | 0x39 => self.add_hl_rr(opcode),
            0xE8 => self.add_sp_e8(bus),
            0xF8 => self.ld_hl_sp_e8(bus),

            // --- Sauts (jumps.rs) ---
            0xC3 => self.jp(bus),
            0xC2 | 0xCA | 0xD2 | 0xDA => self.jp_cc(bus, opcode),
            0xE9 => self.jp_hl(),
            0x18 => self.jr(bus),
            0x20 | 0x28 | 0x30 | 0x38 => self.jr_cc(bus, opcode),
            0xCD => self.call(bus),
            0xC4 | 0xCC | 0xD4 | 0xDC => self.call_cc(bus, opcode),
            0xC9 => self.ret(bus),
            0xC0 | 0xC8 | 0xD0 | 0xD8 => self.ret_cc(bus, opcode),
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => self.rst(bus, opcode),

            // --- Pile (stack.rs) ---
            0xC5 | 0xD5 | 0xE5 | 0xF5 => self.push_rr(bus, opcode),
            0xC1 | 0xD1 | 0xE1 | 0xF1 => self.pop_rr(bus, opcode),

            // --- Préfixe CB : 256 instructions sur les bits (cb.rs) ---
            0xCB => self.cb(bus),

            // --- Interruptions et veille (interrupts.rs) ---
            0xF3 => self.di(),
            0xFB => self.ei(),
            0xD9 => self.reti(bus),
            0x10 => self.stop(bus),

            // --- Divers (misc.rs) ---
            0x2F => self.cpl(),
            0x37 => self.scf(),
            0x3F => self.ccf(),
            0x27 => self.daa(),
            0x07 | 0x0F | 0x17 | 0x1F => self.rotate_a(opcode),

            // --- Les 11 cases vides héritées du Z80 ---
            0xD3 | 0xDB | 0xDD | 0xE3 | 0xE4 | 0xEB | 0xEC | 0xED | 0xF4 | 0xFC | 0xFD => {
                bail!("opcode illégal {opcode:#04X} en {addr:#06X} (la vraie console se figerait)")
            }
        };
        Ok(cycles)
    }

    /// Lit l'octet pointé par PC, puis avance PC.
    fn fetch8(&mut self, bus: &Bus) -> u8 {
        let byte = bus.read8(self.pc);
        if self.halt_bug {
            self.halt_bug = false; // PC reste en place : cet octet sera relu
        } else {
            self.pc = self.pc.wrapping_add(1);
        }
        byte
    }

    /// Lit 2 octets à PC (little-endian), puis avance PC de 2.
    fn fetch16(&mut self, bus: &Bus) -> u16 {
        let low = self.fetch8(bus) as u16;
        let high = self.fetch8(bus) as u16;
        (high << 8) | low
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::bus_with;
    use super::*;

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
    fn opcode_illegal_donne_une_erreur_claire() {
        let mut bus = bus_with(&[0x00, 0xD3]); // 0xD3 n'existe pas sur le SM83
        let mut cpu = Cpu::new();
        cpu.step(&mut bus).unwrap();
        let err = cpu.step(&mut bus).unwrap_err();
        assert_eq!(
            err.to_string(),
            "opcode illégal 0xD3 en 0x0101 (la vraie console se figerait)"
        );
    }

    #[test]
    fn fetch16_lit_en_little_endian() {
        let bus = bus_with(&[0x50, 0x01]);
        let mut cpu = Cpu::new();
        assert_eq!(cpu.fetch16(&bus), 0x0150);
        assert_eq!(cpu.pc, 0x0102);
    }

    #[test]
    fn halt_n_est_pas_une_ld() {
        let mut bus = bus_with(&[0x76]);
        let mut cpu = Cpu::new();
        cpu.step(&mut bus).unwrap();
        assert!(cpu.halted);
    }
}
