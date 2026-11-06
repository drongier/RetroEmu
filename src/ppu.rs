//! PPU (Picture Processing Unit) : la puce qui dessine l'écran.
//!
//! Pour l'instant elle ne dessine rien. Elle possède sa mémoire (VRAM, OAM) et ses registres,
//! et fait avancer LY ligne par ligne. Le rendu des pixels arrive à l'étape 5.

/// Durée d'une ligne, en dots (1 dot = 1 T-cycle).
const DOTS_PER_LINE: u32 = 456;
/// Lignes par image : 144 visibles + 10 de VBlank.
const LINES_PER_FRAME: u8 = 154;
/// Première ligne de VBlank.
const VBLANK_LINE: u8 = 144;

pub struct Ppu {
    /// 8 KiB de mémoire vidéo (0x8000–0x9FFF) : les tuiles et le plan du décor.
    vram: [u8; 0x2000],
    /// 160 octets d'OAM (0xFE00–0xFE9F) : 40 sprites × 4 octets.
    oam: [u8; 0xA0],
    /// Dots écoulés dans la ligne en cours (0 à 455).
    dots: u32,
    lcdc: u8,
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,
}

impl Ppu {
    pub fn new() -> Ppu {
        Ppu {
            vram: [0; 0x2000],
            oam: [0; 0xA0],
            dots: 0,
            lcdc: 0,
            stat: 0,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0,
            obp0: 0,
            obp1: 0,
            wy: 0,
            wx: 0,
        }
    }

    /// Avance de `cycles` dots. Renvoie `true` quand l'écran entre en VBlank.
    pub fn tick(&mut self, cycles: u32) -> bool {
        let mut vblank = false;
        self.dots += cycles;
        while self.dots >= DOTS_PER_LINE {
            self.dots -= DOTS_PER_LINE; // on garde le surplus pour la ligne suivante
            self.ly = (self.ly + 1) % LINES_PER_FRAME;
            if self.ly == VBLANK_LINE {
                vblank = true;
            }
        }
        vblank
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => self.vram[(addr - 0x8000) as usize],
            0xFE00..=0xFE9F => self.oam[(addr - 0xFE00) as usize],
            0xFF40 => self.lcdc,
            0xFF41 => self.stat | 0x80, // le bit 7 n'existe pas et se lit à 1
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0x9FFF => self.vram[(addr - 0x8000) as usize] = value,
            0xFE00..=0xFE9F => self.oam[(addr - 0xFE00) as usize] = value,
            0xFF40 => self.lcdc = value,
            0xFF41 => self.stat = (self.stat & 0x07) | (value & 0x78), // bits 0-2 en lecture seule
            0xFF42 => self.scy = value,
            0xFF43 => self.scx = value,
            0xFF44 => {} // LY est en lecture seule : c'est la PPU qui le fait avancer
            0xFF45 => self.lyc = value,
            0xFF47 => self.bgp = value,
            0xFF48 => self.obp0 = value,
            0xFF49 => self.obp1 = value,
            0xFF4A => self.wy = value,
            0xFF4B => self.wx = value,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ly_avance_toutes_les_456_dots() {
        let mut ppu = Ppu::new();
        ppu.tick(455);
        assert_eq!(ppu.read(0xFF44), 0);
        ppu.tick(1);
        assert_eq!(ppu.read(0xFF44), 1);
    }

    #[test]
    fn vblank_a_la_ligne_144_puis_retour_a_0() {
        let mut ppu = Ppu::new();
        assert!(!ppu.tick(143 * 456));
        assert!(ppu.tick(456));
        assert_eq!(ppu.read(0xFF44), 144);
        ppu.tick(10 * 456);
        assert_eq!(ppu.read(0xFF44), 0);
    }

    #[test]
    fn vram_et_oam_gardent_les_valeurs() {
        let mut ppu = Ppu::new();
        ppu.write(0x8000, 0x12);
        ppu.write(0x9FFF, 0x34);
        ppu.write(0xFE00, 0x56);
        assert_eq!(ppu.read(0x8000), 0x12);
        assert_eq!(ppu.read(0x9FFF), 0x34);
        assert_eq!(ppu.read(0xFE00), 0x56);
    }

    #[test]
    fn ly_est_en_lecture_seule() {
        let mut ppu = Ppu::new();
        ppu.write(0xFF44, 99);
        assert_eq!(ppu.read(0xFF44), 0);
    }
}
