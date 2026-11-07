//! GameBoy : la console entière.
//!
//! Possède le CPU et le Bus, et fait avancer le tout image par image. C'est la seule chose
//! que `main.rs` manipule : il n'a pas besoin de savoir ce qu'il y a dedans.

use anyhow::Result;

use crate::bus::Bus;
use crate::cartridge::Cartridge;
use crate::cpu::Cpu;
use crate::joypad::Button;
use crate::ppu::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// T-cycles dans une image : 154 lignes × 456 dots.
pub const CYCLES_PER_FRAME: u32 = 70_224;

pub struct GameBoy {
    cpu: Cpu,
    bus: Bus,
    /// Cycles déjà exécutés dans l'image en cours. Une image peut déborder un peu
    /// (la dernière instruction finit après la limite) : ce surplus compte pour la suivante.
    frame_cycles: u32,
}

impl GameBoy {
    pub fn new(cartridge: Cartridge) -> GameBoy {
        GameBoy {
            cpu: Cpu::new(),
            bus: Bus::new(cartridge),
            frame_cycles: 0,
        }
    }

    /// Exécute une image complète (70 224 T-cycles), instruction par instruction.
    pub fn run_frame(&mut self) -> Result<()> {
        self.run_frame_until(|_| false)?;
        Ok(())
    }

    /// Comme `run_frame`, mais s'arrête avant d'exécuter une instruction si `stop(pc)` répond
    /// `true`. Renvoie `true` si on s'est arrêté en route ; l'appel suivant reprend l'image là
    /// où elle en était.
    pub fn run_frame_until(&mut self, stop: impl Fn(u16) -> bool) -> Result<bool> {
        while self.frame_cycles < CYCLES_PER_FRAME {
            if stop(self.cpu.pc()) {
                return Ok(true);
            }
            self.step()?;
        }
        self.frame_cycles -= CYCLES_PER_FRAME;
        Ok(false)
    }

    /// Exécute une seule instruction (ou un tour de sommeil, ou une interruption).
    pub fn step(&mut self) -> Result<u32> {
        let spent = self.cpu.step(&mut self.bus)?;
        self.bus.tick(spent);
        self.frame_cycles += spent;
        Ok(spent)
    }

    /// Accès en lecture seule, pour le debugger.
    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// Accès en lecture seule, pour le debugger.
    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    /// L'écran : une teinte (0 = blanc à 3 = noir) par pixel, ligne après ligne.
    pub fn framebuffer(&self) -> &[u8; SCREEN_WIDTH * SCREEN_HEIGHT] {
        self.bus.framebuffer()
    }

    pub fn press(&mut self, button: Button) {
        self.bus.press_button(button);
    }

    pub fn release(&mut self, button: Button) {
        self.bus.release_button(button);
    }

    /// Octets envoyés sur le port série depuis le dernier appel (les tests Blargg y écrivent).
    pub fn take_serial_output(&mut self) -> Vec<u8> {
        self.bus.take_serial_output()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::INT_VBLANK;

    #[test]
    fn une_image_de_nop() {
        // Une ROM pleine de 0x00 : uniquement des NOP, 4 cycles chacun.
        let mut gb = GameBoy::new(Cartridge::from_bytes(vec![0; 0x8000]).unwrap());
        gb.run_frame().unwrap();

        assert_eq!(gb.frame_cycles, 0); // 70 224 / 4 tombe juste
        assert_eq!(gb.bus.read8(0xFF44), 0); // LY a fait le tour complet : 154 lignes
        assert_eq!(gb.bus.read8(0xFF0F) & INT_VBLANK, INT_VBLANK);
    }

    #[test]
    fn une_erreur_du_cpu_remonte() {
        let mut rom = vec![0; 0x8000];
        rom[0x0100] = 0xD3; // opcode inexistant sur le SM83
        let mut gb = GameBoy::new(Cartridge::from_bytes(rom).unwrap());
        assert!(gb.run_frame().is_err());
    }

    #[test]
    fn s_arreter_au_milieu_d_une_image_puis_reprendre() {
        let mut gb = GameBoy::new(Cartridge::from_bytes(vec![0; 0x8000]).unwrap()); // que des NOP
        assert!(gb.run_frame_until(|pc| pc == 0x0110).unwrap());
        assert_eq!(gb.cpu().pc(), 0x0110);
        assert_eq!(gb.frame_cycles, 16 * 4); // 16 NOP exécutés

        assert!(!gb.run_frame_until(|_| false).unwrap()); // finit la même image
        assert_eq!(gb.frame_cycles, 0);
    }

    #[test]
    fn l_entry_point_classique_tourne_sans_erreur() {
        // NOP ; JP 0x0150 : le début de presque tous les jeux.
        let mut rom = vec![0; 0x8000];
        rom[0x0100..0x0104].copy_from_slice(&[0x00, 0xC3, 0x50, 0x01]);
        let mut gb = GameBoy::new(Cartridge::from_bytes(rom).unwrap());
        for _ in 0..60 {
            gb.run_frame().unwrap(); // une seconde d'émulation
        }
    }
}
