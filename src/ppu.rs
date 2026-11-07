//! PPU (Picture Processing Unit) : la puce qui dessine l'écran.
//!
//! Elle possède sa mémoire (VRAM, OAM) et ses registres, fait avancer LY ligne par ligne,
//! et dessine chaque ligne dans un framebuffer de 160 × 144 teintes (0 = blanc, 3 = noir).
//!
//! Pour l'instant seul le fond est dessiné. La window et les sprites viendront ensuite.

/// Durée d'une ligne, en dots (1 dot = 1 T-cycle).
const DOTS_PER_LINE: u32 = 456;
/// Lignes par image : 144 visibles + 10 de VBlank.
const LINES_PER_FRAME: u8 = 154;
/// Première ligne de VBlank.
const VBLANK_LINE: u8 = 144;

/// Taille de l'écran, en pixels.
pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

/// Bits de LCDC (0xFF40) utiles au fond.
const LCDC_BG_ENABLE: u8 = 0x01; // 0 : fond tout blanc
const LCDC_BG_MAP: u8 = 0x08; //    0 : carte en 9800, 1 : carte en 9C00
const LCDC_TILE_DATA: u8 = 0x10; // 0 : mode 8800 (numéros signés), 1 : mode 8000

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
    /// L'image en cours de dessin : une teinte (0 à 3) par pixel, ligne après ligne.
    framebuffer: [u8; SCREEN_WIDTH * SCREEN_HEIGHT],
}

impl Ppu {
    pub fn new() -> Ppu {
        Ppu {
            vram: [0; 0x2000],
            oam: [0; 0xA0],
            dots: 0,
            lcdc: 0x91, // valeurs laissées par la boot ROM : écran allumé, fond actif, mode 8000
            stat: 0,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0xFC,
            obp0: 0,
            obp1: 0,
            wy: 0,
            wx: 0,
            framebuffer: [0; SCREEN_WIDTH * SCREEN_HEIGHT],
        }
    }

    /// La dernière image dessinée, pour l'affichage.
    pub fn framebuffer(&self) -> &[u8; SCREEN_WIDTH * SCREEN_HEIGHT] {
        &self.framebuffer
    }

    /// Avance de `cycles` dots. Renvoie `true` quand l'écran entre en VBlank.
    pub fn tick(&mut self, cycles: u32) -> bool {
        let mut vblank = false;
        self.dots += cycles;
        while self.dots >= DOTS_PER_LINE {
            self.dots -= DOTS_PER_LINE; // on garde le surplus pour la ligne suivante
            // La ligne qui se termine était visible : on la dessine avant de passer à la suivante.
            if self.ly < VBLANK_LINE {
                self.render_line();
            }
            self.ly = (self.ly + 1) % LINES_PER_FRAME;
            if self.ly == VBLANK_LINE {
                vblank = true;
            }
        }
        vblank
    }

    /// Dessine la ligne LY dans le framebuffer.
    fn render_line(&mut self) {
        let start = self.ly as usize * SCREEN_WIDTH;
        for x in 0..SCREEN_WIDTH {
            self.framebuffer[start + x] = if self.lcdc & LCDC_BG_ENABLE != 0 {
                shade(self.bgp, self.background_color(x as u8))
            } else {
                0 // fond désactivé : blanc, quelle que soit la palette
            };
        }
    }

    /// Numéro de couleur (0 à 3) du fond au pixel `x` de la ligne LY.
    fn background_color(&self, x: u8) -> u8 {
        // ① Où suis-je dans la carte ? Les u8 font le tour à 256 tout seuls.
        let bx = x.wrapping_add(self.scx);
        let by = self.ly.wrapping_add(self.scy);
        // ② Quelle tuile ? La carte fait 32 cases de large.
        let map = if self.lcdc & LCDC_BG_MAP != 0 { 0x9C00 } else { 0x9800 };
        let tile = self.vram_read(map + (by / 8) as u16 * 32 + (bx / 8) as u16);
        // ③ et ④ Quelle ligne, quel pixel de la tuile ?
        self.tile_pixel(tile, by % 8, bx % 8)
    }

    /// Adresse des 16 octets de la tuile `tile`, selon le mode choisi par LCDC.
    fn tile_address(&self, tile: u8) -> u16 {
        if self.lcdc & LCDC_TILE_DATA != 0 {
            0x8000 + tile as u16 * 16
        } else {
            // Numéro signé autour de 0x9000 : 0x80 (-128) donne 0x8800.
            0x9000u16.wrapping_add((tile as i8 as i16 * 16) as u16)
        }
    }

    /// Numéro de couleur du pixel (`col`, `row`) de la tuile `tile`.
    fn tile_pixel(&self, tile: u8, row: u8, col: u8) -> u8 {
        let addr = self.tile_address(tile) + row as u16 * 2;
        let low = self.vram_read(addr); //      les bits bas des 8 pixels
        let high = self.vram_read(addr + 1); // les bits hauts
        let bit = 7 - col; // le pixel 0 est le bit 7
        ((high >> bit) & 1) << 1 | ((low >> bit) & 1)
    }

    fn vram_read(&self, addr: u16) -> u8 {
        self.vram[(addr - 0x8000) as usize]
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

/// Teinte (0 à 3) du numéro de couleur `color` dans une palette (BGP, OBP0, OBP1) :
/// 2 bits par couleur, la couleur 0 dans les bits 1-0.
fn shade(palette: u8, color: u8) -> u8 {
    (palette >> (color * 2)) & 0x03
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

    /// Écrit la ligne « caisse » de l'artefact (DB A5) comme ligne 2 de la tuile en `addr`.
    fn write_crate_row(ppu: &mut Ppu, addr: u16) {
        ppu.write(addr + 4, 0xDB);
        ppu.write(addr + 5, 0xA5);
    }

    #[test]
    fn decode_une_ligne_de_tuile() {
        let mut ppu = Ppu::new(); // LCDC = 0x91 : mode 8000
        write_crate_row(&mut ppu, 0x8030); // tuile 3
        let row: Vec<u8> = (0..8).map(|col| ppu.tile_pixel(3, 2, col)).collect();
        assert_eq!(row, [3, 1, 2, 1, 1, 2, 1, 3]);
    }

    #[test]
    fn les_deux_modes_d_adressage() {
        let mut ppu = Ppu::new();
        assert_eq!(ppu.tile_address(0x00), 0x8000);
        assert_eq!(ppu.tile_address(0x80), 0x8800);
        ppu.write(0xFF40, 0x81); // LCDC bit 4 = 0 : mode 8800
        assert_eq!(ppu.tile_address(0x00), 0x9000);
        assert_eq!(ppu.tile_address(0x7F), 0x97F0);
        assert_eq!(ppu.tile_address(0x80), 0x8800); // -128
        assert_eq!(ppu.tile_address(0xFF), 0x8FF0); // -1
    }

    #[test]
    fn la_palette_choisit_la_teinte() {
        assert_eq!(shade(0xE4, 1), 1); // palette « normale »
        assert_eq!(shade(0x1B, 0), 3); // palette inversée
        assert_eq!(shade(0x00, 3), 0); // tout blanc
    }

    #[test]
    fn l_exemple_chiffre_de_l_artefact() {
        // Pixel x = 10 de la ligne 20, SCX = 100, SCY = 6 : case (13, 3) → 986D → tuile 3.
        let mut ppu = Ppu::new();
        ppu.write(0x986D, 3);
        write_crate_row(&mut ppu, 0x8030);
        ppu.write(0xFF42, 6); // SCY
        ppu.write(0xFF43, 100); // SCX
        ppu.write(0xFF47, 0xE4); // BGP
        ppu.ly = 20;
        ppu.render_line();
        assert_eq!(ppu.framebuffer()[20 * SCREEN_WIDTH + 10], 1); // gris clair
    }

    #[test]
    fn le_scroll_fait_le_tour_de_la_carte() {
        // La case (0, 0) de la carte réapparaît à droite quand SCX vaut 250.
        let mut ppu = Ppu::new();
        ppu.write(0x9800, 1); // case (0, 0) : tuile 1
        for addr in 0x8010..0x8020 {
            ppu.write(addr, 0xFF); // tuile 1 : couleur 3 partout
        }
        ppu.write(0xFF43, 250);
        ppu.render_line();
        let line = &ppu.framebuffer()[..SCREEN_WIDTH];
        assert_eq!(line[5], 0); // bx = 255 : dernière case de la carte, vide
        assert_eq!(line[6], 3); // bx = 0 : on a fait le tour
        assert_eq!(line[13], 3); // bx = 7
        assert_eq!(line[14], 0); // bx = 8 : case suivante
    }

    #[test]
    fn chaque_ligne_est_dessinee_a_sa_fin() {
        let mut ppu = Ppu::new();
        for addr in 0x8000..0x8010 {
            ppu.write(addr, 0xFF); // tuile 0 noire : toute la carte (que des 0) est noire
        }
        ppu.tick(455);
        assert_eq!(ppu.framebuffer()[0], 0); // la ligne 0 n'est pas finie : rien de dessiné
        ppu.tick(1);
        assert_eq!(ppu.framebuffer()[0], 3); // ligne 0 terminée : dessinée
        assert_eq!(ppu.framebuffer()[SCREEN_WIDTH], 0); // la ligne 1 pas encore

        ppu.tick(143 * 456); // jusqu'au VBlank
        assert!(ppu.framebuffer().iter().all(|&s| s == 3)); // les 144 lignes sont dessinées
    }

    #[test]
    fn fond_desactive_tout_blanc() {
        let mut ppu = Ppu::new();
        for addr in 0x8000..0x8010 {
            ppu.write(addr, 0xFF); // tuile 0 : noire, et toute la carte vaut 0
        }
        ppu.write(0xFF40, 0x90); // LCDC bit 0 = 0
        ppu.render_line();
        assert!(ppu.framebuffer()[..SCREEN_WIDTH].iter().all(|&s| s == 0));
    }
}
