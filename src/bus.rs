//! Bus : relie le CPU à tout le reste.
//!
//! Le CPU ne connaît que des adresses. Le bus regarde l'adresse et transmet la lecture ou
//! l'écriture à la puce qui l'occupe (la memory map). Il distribue aussi les cycles aux puces
//! et range leurs demandes d'interruption dans IF.

use crate::cartridge::Cartridge;
use crate::joypad::{Button, Joypad};
use crate::ppu::Ppu;
use crate::timer::Timer;

/// Bits des registres IF et IE : un par source d'interruption.
pub const INT_VBLANK: u8 = 0x01;
pub const INT_STAT: u8 = 0x02;
pub const INT_TIMER: u8 = 0x04;
pub const INT_SERIAL: u8 = 0x08;
pub const INT_JOYPAD: u8 = 0x10;

pub struct Bus {
    cartridge: Cartridge,
    ppu: Ppu,
    timer: Timer,
    joypad: Joypad,
    /// 8 KiB de RAM de travail (0xC000–0xDFFF).
    wram: [u8; 0x2000],
    /// 127 octets de RAM rapide (0xFF80–0xFFFE).
    hram: [u8; 0x7F],
    /// Interruptions autorisées par le jeu (0xFFFF).
    ie: u8,
    /// Interruptions en attente (0xFF0F). `if` est un mot-clé de Rust, d'où le `_`.
    if_: u8,
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Bus {
        Bus {
            cartridge,
            ppu: Ppu::new(),
            timer: Timer::new(),
            joypad: Joypad::new(),
            wram: [0; 0x2000],
            hram: [0; 0x7F],
            ie: 0,
            if_: 0,
        }
    }

    pub fn read8(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.cartridge.read_rom(addr),
            0x8000..=0x9FFF => self.ppu.read(addr),
            0xA000..=0xBFFF => self.cartridge.read_ram(addr),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize], // echo : miroir de C000–DDFF
            0xFE00..=0xFE9F => self.ppu.read(addr),
            0xFEA0..=0xFEFF => 0xFF, // zone inutilisable : rien n'est branché
            0xFF00 => self.joypad.read(),
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.if_ | 0xE0, // les bits 5-7 n'existent pas et se lisent à 1
            0xFF40..=0xFF4B => self.ppu.read(addr),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie,
            _ => 0xFF, // registres pas encore émulés (son, port série...)
        }
    }

    pub fn write8(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.cartridge.write_rom(addr, value),
            0x8000..=0x9FFF => self.ppu.write(addr, value),
            0xA000..=0xBFFF => self.cartridge.write_ram(addr, value),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = value,
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize] = value,
            0xFE00..=0xFE9F => self.ppu.write(addr, value),
            0xFEA0..=0xFEFF => {}
            0xFF00 => self.joypad.write(value),
            0xFF04..=0xFF07 => self.timer.write(addr, value),
            0xFF0F => self.if_ = value & 0x1F,
            0xFF46 => self.dma(value), // avant la plage de la PPU : le premier bras qui correspond gagne
            0xFF40..=0xFF4B => self.ppu.write(addr, value),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = value,
            0xFFFF => self.ie = value,
            _ => {}
        }
    }

    /// Lit 2 octets en little-endian : l'octet de poids faible est à `addr`.
    pub fn read16(&self, addr: u16) -> u16 {
        let low = self.read8(addr) as u16;
        let high = self.read8(addr.wrapping_add(1)) as u16;
        (high << 8) | low
    }

    pub fn write16(&mut self, addr: u16, value: u16) {
        self.write8(addr, value as u8);
        self.write8(addr.wrapping_add(1), (value >> 8) as u8);
    }

    /// Avance toutes les puces de `cycles` T-cycles et range leurs interruptions dans IF.
    pub fn tick(&mut self, cycles: u32) {
        if self.ppu.tick(cycles) {
            self.request_interrupt(INT_VBLANK);
        }
        if self.timer.tick(cycles) {
            self.request_interrupt(INT_TIMER);
        }
    }

    pub fn request_interrupt(&mut self, interrupt: u8) {
        self.if_ |= interrupt;
    }

    /// Interruptions à la fois demandées (IF) et autorisées (IE) : celles qui peuvent sonner.
    pub fn pending_interrupts(&self) -> u8 {
        self.ie & self.if_ & 0x1F
    }

    /// Efface une demande dans IF : le CPU l'a prise en compte.
    pub fn clear_interrupt(&mut self, interrupt: u8) {
        self.if_ &= !interrupt;
    }

    pub fn press_button(&mut self, button: Button) {
        if self.joypad.press(button) {
            self.request_interrupt(INT_JOYPAD);
        }
    }

    pub fn release_button(&mut self, button: Button) {
        self.joypad.release(button);
    }

    /// DMA : copie 160 octets depuis l'adresse `value` × 0x100 vers l'OAM.
    /// Sur la vraie console, la copie prend 640 T-cycles ; ici elle est instantanée.
    fn dma(&mut self, value: u8) {
        let source = (value as u16) << 8;
        for i in 0..0xA0 {
            let byte = self.read8(source + i);
            self.ppu.write(0xFE00 + i, byte);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus() -> Bus {
        Bus::new(Cartridge::from_bytes(vec![0; 0x8000]).unwrap())
    }

    #[test]
    fn echo_ram_est_un_miroir_de_la_wram() {
        let mut bus = bus();
        bus.write8(0xC005, 0x42);
        assert_eq!(bus.read8(0xE005), 0x42);
        bus.write8(0xE010, 0x99);
        assert_eq!(bus.read8(0xC010), 0x99);
    }

    #[test]
    fn read16_est_little_endian() {
        let mut bus = bus();
        bus.write8(0xC000, 0x34);
        bus.write8(0xC001, 0x12);
        assert_eq!(bus.read16(0xC000), 0x1234);
        bus.write16(0xFF80, 0xBEEF);
        assert_eq!(bus.read8(0xFF80), 0xEF);
        assert_eq!(bus.read8(0xFF81), 0xBE);
    }

    #[test]
    fn aiguille_vers_les_bonnes_puces() {
        let mut bus = bus();
        bus.write8(0x8000, 0x11); // VRAM → PPU
        assert_eq!(bus.read8(0x8000), 0x11);
        bus.tick(256); // DIV → Timer
        assert_eq!(bus.read8(0xFF04), 1);
        assert_eq!(bus.read8(0xFEA0), 0xFF); // zone inutilisable
    }

    #[test]
    fn vblank_leve_le_bit_0_de_if() {
        let mut bus = bus();
        assert_eq!(bus.read8(0xFF0F) & INT_VBLANK, 0);
        bus.tick(144 * 456);
        assert_eq!(bus.read8(0xFF0F) & INT_VBLANK, INT_VBLANK);
    }

    #[test]
    fn dma_copie_160_octets_dans_l_oam() {
        let mut bus = bus();
        bus.write8(0xC100, 0xAA);
        bus.write8(0xC19F, 0xBB);
        bus.write8(0xFF46, 0xC1);
        assert_eq!(bus.read8(0xFE00), 0xAA);
        assert_eq!(bus.read8(0xFE9F), 0xBB);
    }

    #[test]
    fn appui_bouton_leve_l_interruption_joypad() {
        let mut bus = bus();
        bus.press_button(Button::Start);
        assert_eq!(bus.read8(0xFF0F) & INT_JOYPAD, INT_JOYPAD);
    }
}
