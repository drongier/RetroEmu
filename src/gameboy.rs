//! GameBoy : la console entière.
//!
//! Possède le CPU et le Bus, et fait avancer le tout image par image. C'est la seule chose
//! que `main.rs` manipule : il n'a pas besoin de savoir ce qu'il y a dedans.

use anyhow::Result;

use crate::bus::Bus;
use crate::cartridge::Cartridge;
use crate::cpu::Cpu;
use crate::joypad::Button;

/// T-cycles dans une image : 154 lignes × 456 dots.
pub const CYCLES_PER_FRAME: u32 = 70_224;

pub struct GameBoy {
    cpu: Cpu,
    bus: Bus,
    /// Cycles exécutés en trop à la fin de l'image précédente, déjà comptés pour la suivante.
    surplus: u32,
}

impl GameBoy {
    pub fn new(cartridge: Cartridge) -> GameBoy {
        GameBoy {
            cpu: Cpu::new(),
            bus: Bus::new(cartridge),
            surplus: 0,
        }
    }

    /// Exécute une image complète (70 224 T-cycles), instruction par instruction.
    pub fn run_frame(&mut self) -> Result<()> {
        let mut cycles = self.surplus;
        while cycles < CYCLES_PER_FRAME {
            let spent = self.cpu.step(&mut self.bus)?;
            self.bus.tick(spent);
            cycles += spent;
        }
        self.surplus = cycles - CYCLES_PER_FRAME;
        Ok(())
    }

    pub fn press(&mut self, button: Button) {
        self.bus.press_button(button);
    }

    pub fn release(&mut self, button: Button) {
        self.bus.release_button(button);
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

        assert_eq!(gb.surplus, 0); // 70 224 / 4 tombe juste
        assert_eq!(gb.bus.read8(0xFF44), 0); // LY a fait le tour complet : 154 lignes
        assert_eq!(gb.bus.read8(0xFF0F) & INT_VBLANK, INT_VBLANK);
    }

    #[test]
    fn une_erreur_du_cpu_remonte() {
        let mut rom = vec![0; 0x8000];
        rom[0x0100] = 0xC3; // JP : pas encore supporté
        let mut gb = GameBoy::new(Cartridge::from_bytes(rom).unwrap());
        assert!(gb.run_frame().is_err());
    }
}
