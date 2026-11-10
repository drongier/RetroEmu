//! PPU (Picture Processing Unit) : la puce qui dessine l'écran.
//!
//! Elle possède sa mémoire (VRAM, OAM) et ses registres, fait avancer LY ligne par ligne,
//! et dessine chaque ligne dans un framebuffer de 160 × 144 teintes (0 = blanc, 3 = noir).
//!
//! Pour l'instant le fond et la window sont dessinés. Les sprites viendront ensuite.

/// Durée d'une ligne, en dots (1 dot = 1 T-cycle).
const DOTS_PER_LINE: u32 = 456;
/// Lignes par image : 144 visibles + 10 de VBlank.
const LINES_PER_FRAME: u8 = 154;
/// Première ligne de VBlank.
const VBLANK_LINE: u8 = 144;
/// Fin du mode 2 (OAM scan) et du mode 3 (dessin), en dots depuis le début de la ligne.
/// Le mode 3 dure 172 dots au minimum ; on ignore les retards dus aux sprites et au scroll.
const OAM_SCAN_END: u32 = 80;
const DRAWING_END: u32 = OAM_SCAN_END + 172;

/// Ce que fait la PPU en ce moment. La valeur de chaque mode est celle qu'affiche STAT.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    HBlank = 0,
    VBlank = 1,
    OamScan = 2,
    Drawing = 3,
}

use crate::bus::{INT_STAT, INT_VBLANK};

/// Taille de l'écran, en pixels.
pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

/// Bits de LCDC (0xFF40).
const LCDC_LCD_ON: u8 = 0x80; //     0 : écran éteint, la PPU est à l'arrêt
const LCDC_BG_ENABLE: u8 = 0x01; // 0 : fond tout blanc
const LCDC_OBJ_ENABLE: u8 = 0x02; // 1 : sprites affichés
const LCDC_OBJ_SIZE: u8 = 0x04; //   0 : sprites 8 × 8, 1 : sprites 8 × 16
const LCDC_BG_MAP: u8 = 0x08; //    0 : carte en 9800, 1 : carte en 9C00
const LCDC_TILE_DATA: u8 = 0x10; // 0 : mode 8800 (numéros signés), 1 : mode 8000
const LCDC_WINDOW_ENABLE: u8 = 0x20; // 1 : window affichée
const LCDC_WINDOW_MAP: u8 = 0x40; // 0 : carte en 9800, 1 : carte en 9C00

/// Au-delà, la window commence après le bord droit de l'écran (WX − 7 ≥ 160).
const WX_OFF_SCREEN: u8 = 167;

/// Bits de STAT (0xFF41).
const STAT_LYC_EQUAL: u8 = 0x04; // LY == LYC (calculé par la PPU)
const STAT_HBLANK_INT: u8 = 0x08; // « préviens-moi en mode 0 »
const STAT_VBLANK_INT: u8 = 0x10; // « … en mode 1 »
const STAT_OAM_INT: u8 = 0x20; //    « … en mode 2 »
const STAT_LYC_INT: u8 = 0x40; //    « … quand LY == LYC »

/// Au plus 10 sprites par ligne : les suivants dans l'OAM sont ignorés.
const MAX_SPRITES_PER_LINE: usize = 10;

// Les bits des attributs d'un sprite (octet 3 de sa fiche).
const ATTR_PALETTE: u8 = 0x10; //   0 : OBP0, 1 : OBP1
const ATTR_FLIP_X: u8 = 0x20; //    miroir horizontal
const ATTR_FLIP_Y: u8 = 0x40; //    miroir vertical
const ATTR_BEHIND_BG: u8 = 0x80; // derrière les couleurs 1 à 3 du fond

/// Une fiche de l'OAM (4 octets), telle qu'elle est écrite par le jeu.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Sprite {
    /// Ligne de l'écran + 16 : 16 veut dire « collé en haut ».
    y: u8,
    /// Colonne de l'écran + 8 : 8 veut dire « collé à gauche ».
    x: u8,
    tile: u8,
    attrs: u8,
}

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
    /// Le fil STAT au dot précédent : l'interruption ne part que quand il passe de 0 à 1.
    stat_line: bool,
    /// LY a atteint WY pendant cette image : la window peut apparaître.
    window_y_reached: bool,
    /// Compteur de lignes de la window : n'avance que sur les lignes où elle est dessinée.
    window_line: u8,
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
            stat_line: false,
            window_y_reached: false,
            window_line: 0,
            framebuffer: [0; SCREEN_WIDTH * SCREEN_HEIGHT],
        }
    }

    /// La dernière image dessinée, pour l'affichage.
    pub fn framebuffer(&self) -> &[u8; SCREEN_WIDTH * SCREEN_HEIGHT] {
        &self.framebuffer
    }

    /// Avance de `cycles` dots. Renvoie les interruptions demandées, avec les bits de IF
    /// (`INT_VBLANK`, `INT_STAT`), ou 0 si aucune.
    pub fn tick(&mut self, cycles: u32) -> u8 {
        if !self.lcd_on() {
            return 0; // écran éteint : le temps passe, mais la PPU ne bouge pas
        }
        let mut interrupts = 0;
        for _ in 0..cycles {
            interrupts |= self.tick_dot();
        }
        interrupts
    }

    /// Avance d'un dot et réagit aux changements de mode et de ligne.
    fn tick_dot(&mut self) -> u8 {
        let mut interrupts = 0;
        self.dots += 1;
        if self.dots == DRAWING_END && self.ly < VBLANK_LINE {
            self.render_line(); // fin du mode 3 : la ligne est dessinée, on entre en HBlank
        }
        if self.dots == DOTS_PER_LINE {
            self.dots = 0;
            self.ly = (self.ly + 1) % LINES_PER_FRAME;
            if self.ly == VBLANK_LINE {
                interrupts |= INT_VBLANK;
                self.reset_window(); // la prochaine image repart de la ligne 0 de la window
            }
        }
        // Front montant du fil STAT : il vient de passer de 0 à 1.
        let line = self.stat_line();
        if line && !self.stat_line {
            interrupts |= INT_STAT;
        }
        self.stat_line = line;
        interrupts
    }

    /// Le fil STAT : un OU entre les quatre sources que le jeu a activées dans STAT.
    fn stat_line(&self) -> bool {
        let mode_source = match self.mode() {
            Mode::HBlank => STAT_HBLANK_INT,
            Mode::VBlank => STAT_VBLANK_INT,
            Mode::OamScan => STAT_OAM_INT,
            Mode::Drawing => 0, // le mode 3 n'a pas de source
        };
        let lyc_source = self.ly == self.lyc && self.stat & STAT_LYC_INT != 0;
        self.stat & mode_source != 0 || lyc_source
    }

    /// Le mode se déduit de la position dans l'image : pas besoin de le stocker.
    fn mode(&self) -> Mode {
        if !self.lcd_on() {
            Mode::HBlank // écran éteint : STAT affiche le mode 0
        } else if self.ly >= VBLANK_LINE {
            Mode::VBlank
        } else if self.dots < OAM_SCAN_END {
            Mode::OamScan
        } else if self.dots < DRAWING_END {
            Mode::Drawing
        } else {
            Mode::HBlank
        }
    }

    /// Le CPU peut toucher la VRAM sauf pendant le dessin (mode 3).
    fn vram_accessible(&self) -> bool {
        self.mode() != Mode::Drawing
    }

    /// Le CPU peut toucher l'OAM en HBlank et en VBlank (modes 0 et 1) : le bit 1 du mode à 0.
    fn oam_accessible(&self) -> bool {
        matches!(self.mode(), Mode::HBlank | Mode::VBlank)
    }

    /// Écriture du DMA dans l'OAM : le DMA a son propre accès, il n'est jamais bloqué.
    pub fn write_oam_dma(&mut self, index: usize, value: u8) {
        self.oam[index] = value;
    }

    fn lcd_on(&self) -> bool {
        self.lcdc & LCDC_LCD_ON != 0
    }

    /// Écrit LCDC. Éteindre l'écran arrête la PPU au début de la ligne 0 et blanchit l'image ;
    /// au rallumage, elle repart de là.
    fn write_lcdc(&mut self, value: u8) {
        let was_on = self.lcd_on();
        self.lcdc = value;
        if was_on && !self.lcd_on() {
            self.ly = 0;
            self.dots = 0;
            self.stat_line = false;
            self.reset_window();
            self.framebuffer.fill(0);
        }
    }

    fn reset_window(&mut self) {
        self.window_y_reached = false;
        self.window_line = 0;
    }

    /// Dessine la ligne LY dans le framebuffer.
    fn render_line(&mut self) {
        if self.ly == self.wy {
            self.window_y_reached = true; // retenu jusqu'à la fin de l'image
        }
        let bg_enabled = self.lcdc & LCDC_BG_ENABLE != 0; // sur DMG : fond ET window
        let window_shown = bg_enabled
            && self.lcdc & LCDC_WINDOW_ENABLE != 0
            && self.window_y_reached
            && self.wx < WX_OFF_SCREEN;
        // Colonne de l'écran où la window commence. En i16 : avec WX < 7, elle est négative.
        let window_x = self.wx as i16 - 7;

        // Les sprites de la ligne, rangés par priorité : X croissant. Le tri de Rust est
        // stable : à X égal, ils gardent l'ordre de l'OAM.
        let mut sprites = if self.lcdc & LCDC_OBJ_ENABLE != 0 { self.scan_oam() } else { Vec::new() };
        sprites.sort_by_key(|s| s.x);

        let start = self.ly as usize * SCREEN_WIDTH;
        for x in 0..SCREEN_WIDTH {
            // ① Le numéro de couleur du fond ou de la window. Fond désactivé : couleur 0.
            let bg_color = if !bg_enabled {
                0
            } else if window_shown && x as i16 >= window_x {
                self.window_color((x as i16 - window_x) as u8)
            } else {
                self.background_color(x as u8)
            };
            // ② Le premier sprite, par priorité, qui n'est pas transparent ici.
            let sprite = sprites.iter().find_map(|s| self.sprite_color(s, x as u8).map(|c| (s, c)));
            // ③ La teinte : celle du sprite, sauf s'il passe derrière une couleur 1-3 du fond.
            self.framebuffer[start + x] = match sprite {
                Some((s, color)) if s.attrs & ATTR_BEHIND_BG == 0 || bg_color == 0 => {
                    let palette = if s.attrs & ATTR_PALETTE != 0 { self.obp1 } else { self.obp0 };
                    shade(palette, color)
                }
                // Fond désactivé : blanc, quelle que soit la palette.
                _ => if bg_enabled { shade(self.bgp, bg_color) } else { 0 },
            };
        }
        if window_shown {
            self.window_line += 1; // la window a été dessinée sur cette ligne
        }
    }

    /// Hauteur des sprites, la même pour les 40 : 8 ou 16 lignes selon LCDC bit 2.
    fn sprite_height(&self) -> u8 {
        if self.lcdc & LCDC_OBJ_SIZE != 0 { 16 } else { 8 }
    }

    /// Le scan du mode 2 : les 10 premiers sprites de l'OAM qui croisent la ligne LY.
    /// Seul Y compte : un sprite caché à gauche (X = 0) prend quand même une place.
    fn scan_oam(&self) -> Vec<Sprite> {
        // En u16 : y + 16 dépasse 255 pour les dernières lignes.
        let line = self.ly as u16 + 16;
        let height = self.sprite_height() as u16;
        self.oam
            .chunks_exact(4)
            .map(|b| Sprite { y: b[0], x: b[1], tile: b[2], attrs: b[3] })
            .filter(|s| s.y as u16 <= line && line < s.y as u16 + height)
            .take(MAX_SPRITES_PER_LINE)
            .collect()
    }

    /// Numéro de couleur du sprite au pixel `x` de la ligne LY, ou `None` s'il ne couvre
    /// pas ce pixel ou s'il y est transparent (couleur 0). Le sprite vient de `scan_oam`.
    fn sprite_color(&self, sprite: &Sprite, x: u8) -> Option<u8> {
        // Colonne dans le sprite. On reste en « +8 » : si le sprite commence à droite
        // de x, la soustraction fait le tour et donne un grand nombre, donc ≥ 8.
        let mut col = (x + 8).wrapping_sub(sprite.x);
        if col >= 8 {
            return None;
        }
        // Ligne dans le sprite : le scan garantit 0 ≤ row < hauteur.
        let height = self.sprite_height();
        let mut row = self.ly + 16 - sprite.y;

        if sprite.attrs & ATTR_FLIP_X != 0 {
            col = 7 - col;
        }
        if sprite.attrs & ATTR_FLIP_Y != 0 {
            row = height - 1 - row; // en 8 × 16, les 16 lignes se retournent d'un bloc
        }
        // En 8 × 16, le bit 0 est ignoré : on part toujours de la tuile paire.
        let tile = if height == 16 { sprite.tile & 0xFE } else { sprite.tile };
        // Toujours le mode 8000. Les lignes 8 à 15 tombent pile dans la tuile suivante,
        // rangée juste après (16 octets plus loin).
        let addr = 0x8000 + tile as u16 * 16 + row as u16 * 2;
        match self.row_pixel(addr, col) {
            0 => None, // la couleur 0 d'un sprite est transparente
            color => Some(color),
        }
    }

    /// Numéro de couleur (0 à 3) du fond au pixel `x` de la ligne LY.
    fn background_color(&self, x: u8) -> u8 {
        // Où suis-je dans la carte ? Les u8 font le tour à 256 tout seuls.
        let bx = x.wrapping_add(self.scx);
        let by = self.ly.wrapping_add(self.scy);
        let map = if self.lcdc & LCDC_BG_MAP != 0 { 0x9C00 } else { 0x9800 };
        self.map_color(map, bx, by)
    }

    /// Numéro de couleur de la window à la colonne `wx` de la window (0 = son bord gauche).
    /// Pas de scroll : sa ligne est donnée par son propre compteur.
    fn window_color(&self, wx: u8) -> u8 {
        let map = if self.lcdc & LCDC_WINDOW_MAP != 0 { 0x9C00 } else { 0x9800 };
        self.map_color(map, wx, self.window_line)
    }

    /// Numéro de couleur au point (`x`, `y`) d'une carte de 32 × 32 tuiles.
    fn map_color(&self, map: u16, x: u8, y: u8) -> u8 {
        // Quelle tuile ? La carte fait 32 cases de large.
        let tile = self.vram_read(map + (y / 8) as u16 * 32 + (x / 8) as u16);
        // Quelle ligne, quel pixel de la tuile ?
        self.tile_pixel(tile, y % 8, x % 8)
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
        self.row_pixel(self.tile_address(tile) + row as u16 * 2, col)
    }

    /// Numéro de couleur du pixel `col` d'une ligne de tuile (2 octets à `addr`).
    fn row_pixel(&self, addr: u16, col: u8) -> u8 {
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
            // Pendant que la PPU s'en sert, le CPU lit 0xFF (le cas `_` plus bas).
            0x8000..=0x9FFF if self.vram_accessible() => self.vram[(addr - 0x8000) as usize],
            0xFE00..=0xFE9F if self.oam_accessible() => self.oam[(addr - 0xFE00) as usize],
            0xFF40 => self.lcdc,
            0xFF41 => {
                let lyc_equal = if self.ly == self.lyc { STAT_LYC_EQUAL } else { 0 };
                0x80 | self.stat | lyc_equal | self.mode() as u8 // le bit 7 n'existe pas : lu à 1
            }
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
            // Pendant que la PPU s'en sert, l'écriture est perdue (le cas `_` plus bas).
            0x8000..=0x9FFF if self.vram_accessible() => self.vram[(addr - 0x8000) as usize] = value,
            0xFE00..=0xFE9F if self.oam_accessible() => self.oam[(addr - 0xFE00) as usize] = value,
            0xFF40 => self.write_lcdc(value),
            0xFF41 => self.stat = value & 0x78, // le jeu n'écrit que les bits 3-6 ; 0-2 viennent de la PPU
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
        assert_eq!(ppu.tick(143 * 456), 0);
        assert_eq!(ppu.tick(456), INT_VBLANK);
        assert_eq!(ppu.read(0xFF44), 144);
        ppu.tick(10 * 456);
        assert_eq!(ppu.read(0xFF44), 0);
    }

    #[test]
    fn vram_et_oam_gardent_les_valeurs() {
        let mut ppu = Ppu::new();
        ppu.tick(252); // HBlank : VRAM et OAM libres
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
    fn chaque_ligne_est_dessinee_a_la_fin_du_mode_3() {
        let mut ppu = Ppu::new();
        for addr in 0x8000..0x8010 {
            ppu.write(addr, 0xFF); // tuile 0 noire : toute la carte (que des 0) est noire
        }
        ppu.tick(251);
        assert_eq!(ppu.framebuffer()[0], 0); // encore en mode 3 : rien de dessiné
        ppu.tick(1);
        assert_eq!(ppu.framebuffer()[0], 3); // dot 252, fin du mode 3 : ligne 0 dessinée
        assert_eq!(ppu.framebuffer()[SCREEN_WIDTH], 0); // la ligne 1 pas encore

        ppu.tick(204 + 143 * 456); // jusqu'au VBlank
        assert!(ppu.framebuffer().iter().all(|&s| s == 3)); // les 144 lignes sont dessinées
    }

    /// Le mode lu dans STAT (bits 0-1).
    fn stat_mode(ppu: &Ppu) -> u8 {
        ppu.read(0xFF41) & 0x03
    }

    #[test]
    fn les_modes_d_une_ligne_puis_le_vblank() {
        let mut ppu = Ppu::new();
        assert_eq!(stat_mode(&ppu), 2); // dot 0 : OAM scan
        ppu.tick(79);
        assert_eq!(stat_mode(&ppu), 2);
        ppu.tick(1);
        assert_eq!(stat_mode(&ppu), 3); // dot 80 : dessin
        ppu.tick(172);
        assert_eq!(stat_mode(&ppu), 0); // dot 252 : HBlank
        ppu.tick(204);
        assert_eq!(stat_mode(&ppu), 2); // ligne 1, dot 0 : on recommence
        ppu.tick(143 * 456);
        assert_eq!(stat_mode(&ppu), 1); // ligne 144 : VBlank, quel que soit le dot
        ppu.tick(300);
        assert_eq!(stat_mode(&ppu), 1);
    }

    #[test]
    fn le_jeu_ne_peut_pas_changer_le_mode() {
        let mut ppu = Ppu::new();
        ppu.write(0xFF45, 5); // LYC ≠ LY : le bit 2 reste à 0
        ppu.write(0xFF41, 0xFF);
        assert_eq!(ppu.read(0xFF41), 0xFA); // 1 | bits 3-6 du jeu | bit 2 à 0 | mode 2
    }

    #[test]
    fn stat_signale_ly_egal_lyc() {
        let mut ppu = Ppu::new();
        ppu.write(0xFF45, 2); // LYC
        assert_eq!(ppu.read(0xFF41) & STAT_LYC_EQUAL, 0); // LY = 0
        ppu.tick(2 * 456);
        assert_eq!(ppu.read(0xFF41) & STAT_LYC_EQUAL, STAT_LYC_EQUAL); // LY = 2
        ppu.tick(456);
        assert_eq!(ppu.read(0xFF41) & STAT_LYC_EQUAL, 0); // LY = 3
    }

    /// Avance de `dots` dots, un par un, et compte les interruptions STAT.
    fn count_stat_interrupts(ppu: &mut Ppu, dots: u32) -> u32 {
        (0..dots).filter(|_| ppu.tick(1) & INT_STAT != 0).count() as u32
    }

    #[test]
    fn interruption_lyc_une_fois_par_image() {
        let mut ppu = Ppu::new();
        ppu.write(0xFF45, 100);
        ppu.write(0xFF41, STAT_LYC_INT);
        assert_eq!(count_stat_interrupts(&mut ppu, 100 * 456 - 1), 0);
        assert_eq!(count_stat_interrupts(&mut ppu, 1), 1); // LY passe à 100
        // Le fil reste à 1 pendant toute la ligne 100 : pas d'autre sonnerie.
        assert_eq!(count_stat_interrupts(&mut ppu, 154 * 456 - 1), 0);
        assert_eq!(count_stat_interrupts(&mut ppu, 1), 1); // image suivante
    }

    #[test]
    fn interruption_hblank_a_chaque_ligne_visible() {
        let mut ppu = Ppu::new();
        ppu.write(0xFF41, STAT_HBLANK_INT);
        assert_eq!(count_stat_interrupts(&mut ppu, 154 * 456), 144); // rien pendant le VBlank
    }

    #[test]
    fn deux_sources_collees_ne_sonnent_qu_une_fois() {
        // Le chronogramme de l'artefact : HBlank (bit 3) et OAM scan (bit 5).
        let mut ppu = Ppu::new();
        ppu.write(0xFF41, STAT_HBLANK_INT | STAT_OAM_INT);
        assert_eq!(count_stat_interrupts(&mut ppu, 1), 1); // déjà en mode 2 : le fil monte
        // Sur deux lignes : 3 → 0 sonne deux fois, 0 → 2 jamais (le fil reste à 1).
        assert_eq!(count_stat_interrupts(&mut ppu, 2 * 456), 2);
    }

    #[test]
    fn vram_bloquee_pendant_le_dessin() {
        let mut ppu = Ppu::new();
        ppu.write(0x8000, 0x12); // mode 2 : la VRAM est libre
        ppu.tick(80); // mode 3
        assert_eq!(ppu.read(0x8000), 0xFF); // lecture bloquée
        ppu.write(0x8000, 0x34); // écriture perdue
        ppu.tick(172); // mode 0
        assert_eq!(ppu.read(0x8000), 0x12);
    }

    #[test]
    fn oam_bloquee_pendant_le_scan_et_le_dessin() {
        let mut ppu = Ppu::new();
        ppu.write(0xFE00, 0x12); // mode 2 : perdue
        assert_eq!(ppu.read(0xFE00), 0xFF);
        ppu.tick(80); // mode 3
        ppu.write(0xFE00, 0x34); // perdue aussi
        ppu.tick(172); // mode 0
        assert_eq!(ppu.read(0xFE00), 0x00);
        ppu.write(0xFE00, 0x56);
        ppu.tick(204 + 143 * 456); // VBlank
        assert_eq!(ppu.read(0xFE00), 0x56);
    }

    #[test]
    fn ecran_eteint_tout_est_libre() {
        let mut ppu = Ppu::new();
        ppu.write(0xFF40, 0x11); // écran éteint
        ppu.write(0xFE00, 0x56);
        assert_eq!(ppu.read(0xFE00), 0x56);
    }

    #[test]
    fn eteindre_l_ecran_arrete_la_ppu() {
        let mut ppu = Ppu::new();
        ppu.tick(50 * 456 + 100); // ligne 50, en plein mode 3
        ppu.write(0xFF40, 0x11); // LCDC bit 7 = 0
        assert_eq!(ppu.read(0xFF44), 0);
        assert_eq!(stat_mode(&ppu), 0);
        assert_eq!(ppu.tick(200 * 456), 0); // une image entière passe : rien ne bouge
        assert_eq!(ppu.read(0xFF44), 0);
    }

    #[test]
    fn rallumer_repart_du_debut_de_la_ligne_0() {
        let mut ppu = Ppu::new();
        ppu.tick(50 * 456 + 100);
        ppu.write(0xFF40, 0x11); // éteint
        ppu.write(0xFF40, 0x91); // rallumé
        assert_eq!(stat_mode(&ppu), 2); // dot 0 de la ligne 0 : OAM scan
        ppu.tick(456);
        assert_eq!(ppu.read(0xFF44), 1);
    }

    #[test]
    fn ecran_eteint_tout_blanc() {
        let mut ppu = Ppu::new();
        for addr in 0x8000..0x8010 {
            ppu.write(addr, 0xFF); // tuile 0 noire
        }
        ppu.tick(154 * 456);
        assert!(ppu.framebuffer().iter().all(|&s| s == 3));
        ppu.write(0xFF40, 0x11);
        assert!(ppu.framebuffer().iter().all(|&s| s == 0));
    }

    /// LCDC avec la window : écran, carte window en 9C00, window, mode 8000, fond.
    const LCDC_WITH_WINDOW: u8 = 0xF1;

    /// Tuile 1 : ligne 0 en couleur 3, ligne 1 en couleur 2, ligne 2 en couleur 1, le reste 0.
    /// La carte du fond (9800) ne contient que la tuile 0, toute blanche.
    fn window_test_ppu() -> Ppu {
        let mut ppu = Ppu::new();
        for (row, (low, high)) in [(0xFF, 0xFF), (0x00, 0xFF), (0xFF, 0x00)].into_iter().enumerate() {
            ppu.write(0x8010 + row as u16 * 2, low);
            ppu.write(0x8011 + row as u16 * 2, high);
        }
        ppu.write(0xFF47, 0xE4); // BGP : teinte = couleur
        ppu.write(0xFF40, LCDC_WITH_WINDOW);
        ppu
    }

    /// Teinte du pixel (x, y) de l'écran.
    fn pixel(ppu: &Ppu, x: usize, y: usize) -> u8 {
        ppu.framebuffer()[y * SCREEN_WIDTH + x]
    }

    #[test]
    fn la_window_couvre_a_partir_de_wy_et_wx_moins_7() {
        let mut ppu = window_test_ppu();
        for addr in 0x9C00..0xA000 {
            ppu.write(addr, 1); // toute la carte window : tuile 1
        }
        ppu.write(0xFF4A, 10); // WY
        ppu.write(0xFF4B, 87); // WX : la window commence à x = 80
        ppu.tick(11 * 456);
        assert_eq!(pixel(&ppu, 100, 9), 0); // ligne 9 : au-dessus de WY, le fond
        assert_eq!(pixel(&ppu, 79, 10), 0); // ligne 10, x = 79 : encore le fond
        assert_eq!(pixel(&ppu, 80, 10), 3); // x = 80 : ligne 0 de la window
        assert_eq!(pixel(&ppu, 159, 10), 3); // jusqu'au bord droit
    }

    #[test]
    fn la_window_ne_defile_pas() {
        let mut ppu = window_test_ppu();
        ppu.write(0x9C00, 1); // seule la case (0, 0) de la carte window contient la tuile 1
        ppu.write(0xFF42, 30); // SCY et SCX n'ont aucun effet sur la window
        ppu.write(0xFF43, 50);
        ppu.write(0xFF4A, 0);
        ppu.write(0xFF4B, 7);
        ppu.tick(456);
        assert_eq!(pixel(&ppu, 0, 0), 3); // coin (0, 0) de la carte window
        assert_eq!(pixel(&ppu, 7, 0), 3);
        assert_eq!(pixel(&ppu, 8, 0), 0); // case (1, 0) : tuile 0
    }

    #[test]
    fn le_compteur_de_la_window_saute_les_lignes_cachees() {
        let mut ppu = window_test_ppu();
        ppu.write(0x9C00, 1);
        ppu.write(0xFF4B, 7);
        ppu.tick(456); // ligne 0 : ligne 0 de la window
        ppu.write(0xFF40, LCDC_WITH_WINDOW & !LCDC_WINDOW_ENABLE);
        ppu.tick(456); // ligne 1 : window cachée
        ppu.write(0xFF40, LCDC_WITH_WINDOW);
        ppu.tick(456); // ligne 2
        assert_eq!(pixel(&ppu, 0, 0), 3);
        assert_eq!(pixel(&ppu, 0, 1), 0); // le fond
        assert_eq!(pixel(&ppu, 0, 2), 2); // ligne 1 de la window, pas la 2 (LY − WY)

        ppu.tick(151 * 456); // image suivante
        assert_eq!(pixel(&ppu, 0, 0), 3); // le compteur est reparti de 0
    }

    #[test]
    fn la_window_attend_que_ly_atteigne_wy() {
        let mut ppu = window_test_ppu();
        ppu.write(0x9C00, 1);
        ppu.write(0xFF4A, 5);
        ppu.write(0xFF4B, 7);
        ppu.tick(2 * 456);
        ppu.write(0xFF4A, 0); // trop tard : LY est déjà passé par 0
        ppu.tick(456);
        assert_eq!(pixel(&ppu, 0, 2), 0); // toujours le fond
    }

    #[test]
    fn wx_167_cache_la_window() {
        let mut ppu = window_test_ppu();
        for addr in 0x9C00..0xA000 {
            ppu.write(addr, 1);
        }
        ppu.write(0xFF4B, 167);
        ppu.tick(456);
        assert!(ppu.framebuffer()[..SCREEN_WIDTH].iter().all(|&s| s == 0));
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

    // ---------- Sprites ----------

    /// Écrit la fiche `index` de l'OAM (Y et X bruts, avec leurs +16 et +8).
    fn put_sprite(ppu: &mut Ppu, index: usize, y: u8, x: u8, tile: u8, attrs: u8) {
        for (i, value) in [y, x, tile, attrs].into_iter().enumerate() {
            ppu.write_oam_dma(index * 4 + i, value);
        }
    }

    #[test]
    fn le_scan_garde_les_sprites_qui_croisent_la_ligne() {
        let mut ppu = Ppu::new();
        put_sprite(&mut ppu, 0, 16, 8, 1, 0); //  lignes 0 à 7 : oui
        put_sprite(&mut ppu, 1, 24, 8, 2, 0); //  lignes 8 à 15 : non
        put_sprite(&mut ppu, 2, 9, 8, 3, 0); //   lignes -7 à 0 : oui, sa dernière ligne
        put_sprite(&mut ppu, 3, 8, 8, 4, 0); //   lignes -8 à -1 : non
        let tiles: Vec<u8> = ppu.scan_oam().iter().map(|s| s.tile).collect();
        assert_eq!(tiles, [1, 3]);
    }

    #[test]
    fn le_scan_s_arrete_a_10_sprites() {
        let mut ppu = Ppu::new();
        for i in 0..12 {
            // X = 0 : tous cachés à gauche, mais ils comptent quand même
            put_sprite(&mut ppu, i, 16, 0, i as u8, 0);
        }
        let tiles: Vec<u8> = ppu.scan_oam().iter().map(|s| s.tile).collect();
        assert_eq!(tiles, (0..10).collect::<Vec<u8>>()); // les 10 premiers de l'OAM
    }

    #[test]
    fn en_8x16_un_sprite_couvre_16_lignes() {
        let mut ppu = Ppu::new();
        put_sprite(&mut ppu, 0, 8, 8, 0, 0); // lignes -8 à 7
        assert!(ppu.scan_oam().is_empty()); // en 8 × 8 : lignes -8 à -1, invisible
        ppu.write(0xFF40, 0x91 | LCDC_OBJ_SIZE);
        assert_eq!(ppu.scan_oam().len(), 1);
    }

    /// Tuile 1, ligne 0 : les couleurs 3, 2, 1, puis 0 sur le reste de la ligne.
    fn sprite_test_ppu() -> Ppu {
        let mut ppu = Ppu::new();
        ppu.write(0x8010, 0xA0); // bits bas :  1 0 1 0 0 0 0 0
        ppu.write(0x8011, 0xC0); // bits hauts : 1 1 0 0 0 0 0 0
        ppu
    }

    /// Les couleurs du sprite sur les colonnes `xs` de la ligne LY.
    fn sprite_colors(ppu: &Ppu, xs: std::ops::Range<u8>) -> Vec<Option<u8>> {
        let sprite = ppu.scan_oam()[0];
        xs.map(|x| ppu.sprite_color(&sprite, x)).collect()
    }

    #[test]
    fn un_sprite_couvre_8_colonnes_a_partir_de_x_moins_8() {
        let mut ppu = sprite_test_ppu();
        put_sprite(&mut ppu, 0, 16, 18, 1, 0); // colonnes 10 à 17
        assert_eq!(
            sprite_colors(&ppu, 9..19),
            [None, Some(3), Some(2), Some(1), None, None, None, None, None, None]
        ); //  ↑ x = 9 : avant le sprite     ↑ couleur 0 : transparent   ↑ x = 18 : après
    }

    #[test]
    fn un_sprite_a_moitie_cache_a_gauche() {
        let mut ppu = sprite_test_ppu();
        put_sprite(&mut ppu, 0, 16, 6, 1, 0); // colonnes -2 à 5
        assert_eq!(sprite_colors(&ppu, 0..2), [Some(1), None]); // ses colonnes 2 et 3
    }

    #[test]
    fn le_miroir_x_lit_la_ligne_a_l_envers() {
        let mut ppu = sprite_test_ppu();
        put_sprite(&mut ppu, 0, 16, 8, 1, ATTR_FLIP_X);
        assert_eq!(sprite_colors(&ppu, 5..8), [Some(1), Some(2), Some(3)]);
    }

    #[test]
    fn les_sprites_ignorent_le_mode_8800() {
        let mut ppu = sprite_test_ppu();
        ppu.write(0xFF40, 0x81); // LCDC bit 4 = 0 : le fond passe en mode 8800
        put_sprite(&mut ppu, 0, 16, 8, 1, 0);
        assert_eq!(sprite_colors(&ppu, 0..1), [Some(3)]); // toujours lu en 0x8010
    }

    #[test]
    fn en_8x16_le_miroir_y_retourne_les_deux_tuiles() {
        let mut ppu = Ppu::new();
        ppu.write(0x8020, 0x80); // tuile 2, ligne 0 : pixel 0 de couleur 3
        ppu.write(0x8021, 0x80);
        ppu.write(0x803E, 0x80); // tuile 3, ligne 7 : pixel 0 de couleur 1
        ppu.write(0xFF40, 0x91 | LCDC_OBJ_SIZE);
        // Numéro 3 : le bit 0 est ignoré, le sprite commence à la tuile 2.
        put_sprite(&mut ppu, 0, 16, 8, 3, 0);
        assert_eq!(sprite_colors(&ppu, 0..1), [Some(3)]); // LY 0 = haut de la tuile 2
        put_sprite(&mut ppu, 0, 16, 8, 3, ATTR_FLIP_Y);
        assert_eq!(sprite_colors(&ppu, 0..1), [Some(1)]); // LY 0 = bas de la tuile 3
    }

    const LCDC_WITH_SPRITES: u8 = 0x93; // écran, mode 8000, fond et sprites

    /// Tuile 1 : 3, 2, 1, 0… (comme plus haut). Tuile 2 : une ligne pleine de couleur 1.
    /// OBP0 = 0xE4 : chaque couleur garde sa teinte (0→0, 1→1, 2→2, 3→3).
    fn priority_test_ppu() -> Ppu {
        let mut ppu = sprite_test_ppu();
        ppu.write(0x8020, 0xFF);
        ppu.write(0xFF40, LCDC_WITH_SPRITES);
        ppu.write(0xFF48, 0xE4);
        ppu
    }

    fn first_pixels(ppu: &mut Ppu) -> &[u8] {
        ppu.render_line(); // la ligne 0
        &ppu.framebuffer()[..8]
    }

    #[test]
    fn lcdc_bit_1_cache_les_sprites() {
        let mut ppu = priority_test_ppu();
        put_sprite(&mut ppu, 0, 16, 8, 2, 0);
        assert_eq!(first_pixels(&mut ppu)[0], 1);
        ppu.write(0xFF40, LCDC_WITH_SPRITES & !LCDC_OBJ_ENABLE);
        assert_eq!(first_pixels(&mut ppu)[0], 0);
    }

    #[test]
    fn le_plus_petit_x_passe_devant() {
        let mut ppu = priority_test_ppu();
        put_sprite(&mut ppu, 0, 16, 9, 2, 0); // B : colonnes 1 à 8, OAM #0
        put_sprite(&mut ppu, 1, 16, 8, 1, 0); // A : colonnes 0 à 7, OAM #1
        // A devant ; à x = 3, A est transparent (couleur 0) : on voit B.
        assert_eq!(first_pixels(&mut ppu)[..4], [3, 2, 1, 1]);
    }

    #[test]
    fn a_x_egal_l_ordre_de_l_oam_decide() {
        let mut ppu = priority_test_ppu();
        put_sprite(&mut ppu, 0, 16, 8, 2, 0); // OAM #0 : devant
        put_sprite(&mut ppu, 1, 16, 8, 1, 0);
        assert_eq!(first_pixels(&mut ppu)[..4], [1, 1, 1, 1]);
    }

    #[test]
    fn obp1_avec_le_bit_4() {
        let mut ppu = priority_test_ppu();
        ppu.write(0xFF49, 0x1B); // OBP1 inversée : la couleur 1 donne la teinte 2
        put_sprite(&mut ppu, 0, 16, 8, 2, ATTR_PALETTE);
        assert_eq!(first_pixels(&mut ppu)[0], 2);
    }

    #[test]
    fn derriere_le_fond_seulement_sur_ses_couleurs_1_a_3() {
        let mut ppu = priority_test_ppu();
        ppu.write(0x8000, 0x0F); // fond (tuile 0), ligne 0 : couleur 0 puis 4 × couleur 1
        put_sprite(&mut ppu, 0, 16, 8, 2, 0);
        assert_eq!(first_pixels(&mut ppu), [1, 1, 1, 1, 1, 1, 1, 1]); // devant
        put_sprite(&mut ppu, 0, 16, 8, 2, ATTR_BEHIND_BG);
        // BGP = 0xFC : la couleur 1 du fond donne la teinte 3.
        assert_eq!(first_pixels(&mut ppu), [1, 1, 1, 1, 3, 3, 3, 3]);
    }

    #[test]
    fn fond_desactive_les_sprites_restent_devant() {
        let mut ppu = priority_test_ppu();
        ppu.write(0x8000, 0xFF); // le fond aurait la couleur 1 partout
        ppu.write(0xFF40, LCDC_WITH_SPRITES & !LCDC_BG_ENABLE);
        put_sprite(&mut ppu, 0, 16, 8, 1, ATTR_BEHIND_BG);
        assert_eq!(first_pixels(&mut ppu)[..4], [3, 2, 1, 0]);
    }
}
