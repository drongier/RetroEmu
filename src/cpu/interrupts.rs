//! Les interruptions : DI, EI, RETI, HALT, STOP, et le passage au gestionnaire.
//!
//! Une interruption est servie quand IME = 1 et que (IE & IF) ≠ 0. Le CPU empile alors PC
//! et saute au vecteur de la source : 0x40 + 8 × bit (0x40 VBlank, 0x48 STAT, 0x50 Timer,
//! 0x58 Série, 0x60 Joypad). Si plusieurs sonnent, le bit le plus bas passe en premier.

use super::Cpu;
use crate::bus::Bus;

impl Cpu {
    /// Sert l'interruption en attente la plus prioritaire. Renvoie 20 T-cycles.
    pub(super) fn service_interrupt(&mut self, bus: &mut Bus) -> u32 {
        let bit = bus.pending_interrupts().trailing_zeros() as u16; // le bit 1 le plus bas
        bus.clear_interrupt(1 << bit);
        self.ime = false;
        self.push16(bus, self.pc);
        self.pc = 0x40 + 8 * bit;
        20
    }

    /// DI : coupe les interruptions tout de suite.
    pub(super) fn di(&mut self) -> u32 {
        self.ime = false;
        self.ei_pending = false;
        4
    }

    /// EI : rallume les interruptions, mais seulement après l'instruction suivante.
    pub(super) fn ei(&mut self) -> u32 {
        self.ei_pending = true;
        4
    }

    /// RETI : RET qui rallume les interruptions immédiatement.
    pub(super) fn reti(&mut self, bus: &Bus) -> u32 {
        self.pc = self.pop16(bus);
        self.ime = true;
        16
    }

    /// HALT : met le CPU en veille jusqu'à ce que (IE & IF) ≠ 0, même si IME = 0.
    pub(super) fn halt(&mut self, bus: &Bus) -> u32 {
        if !self.ime && bus.pending_interrupts() != 0 {
            // HALT bug : le CPU ne dort pas, et l'octet suivant sera lu deux fois.
            self.halt_bug = true;
        } else {
            self.halted = true;
        }
        4
    }

    /// STOP : veille profonde, utilisée sur CGB pour changer de vitesse (bonus).
    /// Pour l'instant, on se contente de sauter son second octet.
    pub(super) fn stop(&mut self, bus: &Bus) -> u32 {
        self.fetch8(bus);
        4
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::{bus_with, run};
    use super::*;
    use crate::bus::{INT_TIMER, INT_VBLANK};

    #[test]
    fn une_interruption_saute_au_vecteur() {
        let mut bus = bus_with(&[0x00]);
        let mut cpu = Cpu::new();
        cpu.ime = true;
        bus.write8(0xFFFF, INT_VBLANK);
        bus.request_interrupt(INT_VBLANK);

        assert_eq!(run(&mut cpu, &mut bus, 1), 20);
        assert_eq!(cpu.pc, 0x0040);
        assert_eq!(bus.read16(cpu.sp), 0x0100); // le ticket de retour
        assert!(!cpu.ime); // « ne pas déranger » pendant le gestionnaire
        assert_eq!(bus.read8(0xFF0F) & INT_VBLANK, 0); // « j'ai entendu »
    }

    #[test]
    fn sans_ie_ou_sans_ime_rien_ne_se_passe() {
        let mut bus = bus_with(&[0x00, 0x00]);
        let mut cpu = Cpu::new();
        cpu.ime = true;
        bus.request_interrupt(INT_VBLANK); // sonne, mais pas branchée dans IE
        run(&mut cpu, &mut bus, 1);
        assert_eq!(cpu.pc, 0x0101);

        cpu.ime = false;
        bus.write8(0xFFFF, INT_VBLANK); // branchée, mais IME = 0
        run(&mut cpu, &mut bus, 1);
        assert_eq!(cpu.pc, 0x0102);
    }

    #[test]
    fn vblank_passe_avant_timer() {
        let mut bus = bus_with(&[0x00]);
        let mut cpu = Cpu::new();
        cpu.ime = true;
        bus.write8(0xFFFF, 0x1F);
        bus.request_interrupt(INT_TIMER | INT_VBLANK);

        run(&mut cpu, &mut bus, 1);
        assert_eq!(cpu.pc, 0x0040);
        assert_eq!(bus.read8(0xFF0F) & 0x1F, INT_TIMER); // le timer attend son tour
    }

    #[test]
    fn ei_agit_apres_l_instruction_suivante() {
        let mut bus = bus_with(&[
            0xFB, // 0x0100  EI
            0x00, // 0x0101  NOP   ← exécuté malgré la sonnette
            0x00, // 0x0102        ← l'interruption coupe la parole ici
        ]);
        let mut cpu = Cpu::new();
        bus.write8(0xFFFF, INT_VBLANK);
        bus.request_interrupt(INT_VBLANK);

        run(&mut cpu, &mut bus, 2);
        assert_eq!(cpu.pc, 0x0102);
        run(&mut cpu, &mut bus, 1);
        assert_eq!(cpu.pc, 0x0040);
        assert_eq!(bus.read16(cpu.sp), 0x0102);
    }

    #[test]
    fn di_coupe_tout_de_suite() {
        let mut bus = bus_with(&[0xFB, 0xF3, 0x00]); // EI ; DI ; NOP
        let mut cpu = Cpu::new();
        bus.write8(0xFFFF, INT_VBLANK);
        bus.request_interrupt(INT_VBLANK);
        run(&mut cpu, &mut bus, 3);
        assert_eq!(cpu.pc, 0x0103); // jamais interrompu
        assert!(!cpu.ime);
    }

    #[test]
    fn reti_revient_et_rallume_ime() {
        let mut bus = bus_with(&[0xD9]); // RETI
        let mut cpu = Cpu::new();
        cpu.push16(&mut bus, 0x1234);
        assert_eq!(run(&mut cpu, &mut bus, 1), 16);
        assert_eq!(cpu.pc, 0x1234);
        assert!(cpu.ime);
    }

    #[test]
    fn halt_dort_puis_continue_si_ime_vaut_0() {
        let mut bus = bus_with(&[0x76, 0x00]); // HALT ; NOP
        let mut cpu = Cpu::new();
        bus.write8(0xFFFF, INT_VBLANK);

        run(&mut cpu, &mut bus, 3); // HALT, puis deux tours de sommeil
        assert!(cpu.halted);
        assert_eq!(cpu.pc, 0x0101);

        bus.request_interrupt(INT_VBLANK);
        run(&mut cpu, &mut bus, 1);
        assert!(!cpu.halted);
        assert_eq!(cpu.pc, 0x0102); // réveillé, mais pas de saut au vecteur
    }

    #[test]
    fn halt_bug_lit_l_octet_suivant_deux_fois() {
        let mut bus = bus_with(&[0x76, 0x3C, 0x00]); // HALT ; INC A ; NOP
        let mut cpu = Cpu::new();
        bus.write8(0xFFFF, INT_VBLANK);
        bus.request_interrupt(INT_VBLANK); // déjà en attente, avec IME = 0

        run(&mut cpu, &mut bus, 3);
        assert!(!cpu.halted);
        assert_eq!(cpu.a, 0x03); // INC A exécuté deux fois (A valait 0x01)
        assert_eq!(cpu.pc, 0x0102);
    }

    #[test]
    fn halt_attend_le_vblank() {
        // Le vrai usage : brancher VBlank, rallumer les interruptions, et dormir.
        let mut bus = bus_with(&[
            0x3E, 0x01, // LD A, 0x01
            0xE0, 0xFF, // LDH (0xFF), A   IE = VBlank
            0xFB, //       EI
            0x76, //       HALT
        ]);
        let mut cpu = Cpu::new();
        while cpu.pc != 0x0040 {
            let cycles = cpu.step(&mut bus).unwrap();
            bus.tick(cycles);
        }
        assert_eq!(bus.read8(0xFF44), 144);
        assert_eq!(bus.read16(cpu.sp), 0x0106); // on reviendra après le HALT
    }
}
