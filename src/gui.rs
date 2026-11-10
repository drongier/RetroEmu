//! L'écran virtuel : l'image du jeu (160 × 144) et, juste dessous, la barre de commandes.
//! Tout est en teintes 0 à 3, comme le framebuffer de la PPU : la fenêtre agrandit
//! ensuite le tout d'un seul bloc, avec les 4 verts de la Game Boy.

use crate::font::{self, ADVANCE, GLYPH_HEIGHT};
use crate::ppu;

pub const WIDTH: usize = ppu::SCREEN_WIDTH;
/// La barre commence sous la dernière ligne du jeu.
const BAR_TOP: usize = ppu::SCREEN_HEIGHT;
const BAR_HEIGHT: usize = 16;
pub const HEIGHT: usize = BAR_TOP + BAR_HEIGHT;
/// Ligne du haut des textes de la barre : centrés verticalement.
const TEXT_Y: usize = BAR_TOP + (BAR_HEIGHT - GLYPH_HEIGHT) / 2;

/// Un rectangle de l'écran virtuel, en pixels.
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Rect {
    fn contains(&self, x: usize, y: usize) -> bool {
        (self.x..self.x + self.w).contains(&x) && (self.y..self.y + self.h).contains(&y)
    }
}

/// Les boutons de la barre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BarButton {
    Load,
    Pause,
}

/// Le bouton sous le pixel virtuel (`x`, `y`), s'il y en a un.
pub fn button_at(x: usize, y: usize) -> Option<BarButton> {
    if LOAD_BUTTON.contains(x, y) {
        Some(BarButton::Load)
    } else if PAUSE_BUTTON.contains(x, y) {
        Some(BarButton::Pause)
    } else {
        None
    }
}

const LOAD_BUTTON: Rect = Rect { x: 2, y: BAR_TOP + 2, w: 34, h: 12 };
const PAUSE_BUTTON: Rect = Rect { x: 38, y: BAR_TOP + 2, w: 34, h: 12 };
/// Le nom de la ROM, à droite des boutons.
const NAME_X: usize = 76;

/// Dessine l'écran virtuel complet dans `screen` (WIDTH × HEIGHT teintes).
/// `game` est l'image de la PPU, ou `None` si aucune ROM n'est chargée.
pub fn compose(screen: &mut [u8], game: Option<&[u8]>, rom_name: Option<&str>, paused: bool) {
    // Le haut : l'image du jeu, ou un message d'accueil.
    match game {
        Some(frame) => screen[..frame.len()].copy_from_slice(frame),
        None => {
            screen[..WIDTH * BAR_TOP].fill(0);
            draw_centered(screen, 60, "AUCUNE ROM");
            draw_centered(screen, 76, "LOAD OU GLISSE UN .GB");
        }
    }

    // Le bas : la barre, foncée, avec ses deux boutons clairs et le nom de la ROM.
    screen[WIDTH * BAR_TOP..].fill(3);
    draw_button(screen, &LOAD_BUTTON, "LOAD");
    draw_button(screen, &PAUSE_BUTTON, if paused { "PLAY" } else { "PAUSE" });
    if let Some(name) = rom_name {
        // On coupe au nombre de lettres qui tiennent, plutôt qu'au milieu d'une lettre.
        let max_chars = (WIDTH - NAME_X) / ADVANCE;
        let short: String = name.chars().take(max_chars).collect();
        font::draw_text(screen, WIDTH, NAME_X, TEXT_Y, &short, 0);
    }
}

/// Largeur en pixels d'un texte : 6 par lettre, sans l'espace après la dernière.
fn text_width(text: &str) -> usize {
    (text.chars().count() * ADVANCE).saturating_sub(1)
}

/// Écrit `text` centré horizontalement, à la ligne `y`, en teinte 3.
fn draw_centered(screen: &mut [u8], y: usize, text: &str) {
    let x = WIDTH.saturating_sub(text_width(text)) / 2;
    font::draw_text(screen, WIDTH, x, y, text, 3);
}

fn draw_button(screen: &mut [u8], rect: &Rect, label: &str) {
    for y in rect.y..rect.y + rect.h {
        screen[y * WIDTH + rect.x..y * WIDTH + rect.x + rect.w].fill(1);
    }
    let x = rect.x + (rect.w - text_width(label)) / 2;
    font::draw_text(screen, WIDTH, x, TEXT_Y, label, 3);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_screen() -> Vec<u8> {
        vec![0; WIDTH * HEIGHT]
    }

    #[test]
    fn largeur_d_un_texte() {
        assert_eq!(text_width("LOAD"), 23); // 4 × 6 − 1
        assert_eq!(text_width(""), 0);
    }

    #[test]
    fn l_image_du_jeu_est_copiee_en_haut() {
        let mut screen = blank_screen();
        let frame = [2u8; ppu::SCREEN_WIDTH * ppu::SCREEN_HEIGHT];
        compose(&mut screen, Some(&frame), Some("TETRIS"), false);
        assert!(screen[..WIDTH * BAR_TOP].iter().all(|&s| s == 2));
        assert_eq!(screen[WIDTH * HEIGHT - 1], 3); // le coin bas droit : la barre
    }

    #[test]
    fn le_bouton_pause_devient_play() {
        let mut playing = blank_screen();
        let mut paused = blank_screen();
        compose(&mut playing, None, None, false);
        compose(&mut paused, None, None, true);
        assert_ne!(playing[WIDTH * BAR_TOP..], paused[WIDTH * BAR_TOP..]);
        assert_eq!(playing[..WIDTH * BAR_TOP], paused[..WIDTH * BAR_TOP]); // seul le bas change
    }

    #[test]
    fn un_clic_trouve_son_bouton() {
        assert_eq!(button_at(2, BAR_TOP + 2), Some(BarButton::Load)); // coin haut gauche
        assert_eq!(button_at(35, BAR_TOP + 13), Some(BarButton::Load)); // coin bas droit
        assert_eq!(button_at(36, BAR_TOP + 8), None); // entre les deux boutons
        assert_eq!(button_at(50, BAR_TOP + 8), Some(BarButton::Pause));
        assert_eq!(button_at(50, 20), None); // dans l'image du jeu
    }

    #[test]
    fn un_nom_trop_long_ne_deborde_pas() {
        let mut screen = blank_screen();
        compose(&mut screen, None, Some("UN NOM DE ROM BEAUCOUP TROP LONG"), false);
    }
}
