//! Le navigateur de fichiers : choisir une ROM au clavier ou à la souris. Il se dessine
//! avec la mini police, à la place de l'image du jeu.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::font::{self, ADVANCE};
use crate::gui;
use crate::ppu;

/// Hauteur d'une ligne : 7 pixels de lettres et 2 d'espace. 144 / 9 = 16 lignes.
pub const LINE_HEIGHT: usize = 9;
/// La ligne 0 affiche le dossier ; les 15 suivantes, son contenu.
const VISIBLE: usize = ppu::SCREEN_HEIGHT / LINE_HEIGHT - 1;
/// Lettres par ligne, avec une marge de 2 pixels à gauche.
const MAX_CHARS: usize = (gui::WIDTH - 2) / ADVANCE;

/// Une ligne du navigateur : un dossier (dont « .. ») ou une ROM.
struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

pub struct Browser {
    /// Le dossier affiché, en chemin absolu.
    dir: PathBuf,
    entries: Vec<Entry>,
    /// L'entrée surlignée.
    selected: usize,
    /// La première entrée visible : la liste défile quand elle dépasse 15 lignes.
    scroll: usize,
}

impl Browser {
    /// Ouvre le navigateur sur le dossier `dir`.
    pub fn open(dir: &Path) -> Result<Browser> {
        let dir = dir
            .canonicalize()
            .with_context(|| format!("dossier introuvable : {}", dir.display()))?;
        let entries = list(&dir)?;
        Ok(Browser { dir, entries, selected: 0, scroll: 0 })
    }

    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.scroll_to_selection();
    }

    pub fn down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
        self.scroll_to_selection();
    }

    /// Entrée sur la ligne surlignée : un dossier s'ouvre (et il n'y a rien à charger),
    /// une ROM est renvoyée pour que l'application la charge.
    pub fn enter(&mut self) -> Result<Option<PathBuf>> {
        let Some(entry) = self.entries.get(self.selected) else {
            return Ok(None); // dossier vide
        };
        if entry.is_dir {
            // En cas d'erreur (dossier protégé…), le `?` sort avant : on reste où on est.
            *self = Browser::open(&entry.path)?;
            Ok(None)
        } else {
            Ok(Some(entry.path.clone()))
        }
    }

    /// Un clic sur la ligne `row` de l'écran (0 = le dossier) : la surligne et l'ouvre.
    pub fn click(&mut self, row: usize) -> Result<Option<PathBuf>> {
        if row == 0 || self.scroll + row - 1 >= self.entries.len() {
            return Ok(None);
        }
        self.selected = self.scroll + row - 1;
        self.enter()
    }

    /// Fait défiler la liste juste assez pour que la ligne surlignée reste visible.
    fn scroll_to_selection(&mut self) {
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + VISIBLE {
            self.scroll = self.selected + 1 - VISIBLE;
        }
    }

    /// Dessine le navigateur dans l'écran virtuel, à la place de l'image du jeu.
    pub fn draw(&self, screen: &mut [u8]) {
        // Ligne 0 : le dossier, clair sur fond moyen. Trop long : on garde la fin.
        fill_line(screen, 0, 2);
        let dir = tail(&self.dir.display().to_string(), MAX_CHARS);
        font::draw_text(screen, gui::WIDTH, 2, 1, &dir, 0);

        for row in 0..VISIBLE {
            let y = (row + 1) * LINE_HEIGHT;
            let index = self.scroll + row;
            let selected = index == self.selected;
            // La ligne surlignée est en négatif : texte clair sur fond foncé.
            fill_line(screen, y, if selected { 3 } else { 0 });
            if let Some(entry) = self.entries.get(index) {
                let label = if entry.is_dir { format!("{}/", entry.name) } else { entry.name.clone() };
                let label: String = label.chars().take(MAX_CHARS).collect();
                font::draw_text(screen, gui::WIDTH, 2, y + 1, &label, if selected { 0 } else { 3 });
            }
        }
    }
}

/// Le contenu de `dir` : « .. », puis les dossiers, puis les ROM (.gb et .gbc), chacun
/// par ordre alphabétique. Les fichiers cachés et les autres fichiers sont ignorés.
fn list(dir: &Path) -> Result<Vec<Entry>> {
    let mut dirs = Vec::new();
    let mut roms = Vec::new();
    let items = std::fs::read_dir(dir).with_context(|| format!("impossible de lire {}", dir.display()))?;
    for item in items.flatten() {
        // flatten() saute les entrées illisibles au lieu de tout faire échouer.
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = item.path();
        if path.is_dir() {
            dirs.push(Entry { name, path, is_dir: true });
        } else if is_rom(&path) {
            roms.push(Entry { name, path, is_dir: false });
        }
    }
    dirs.sort_by_key(|entry| entry.name.to_lowercase());
    roms.sort_by_key(|entry| entry.name.to_lowercase());

    let mut entries = Vec::new();
    if let Some(parent) = dir.parent() {
        entries.push(Entry { name: "..".to_string(), path: parent.to_path_buf(), is_dir: true });
    }
    entries.extend(dirs);
    entries.extend(roms);
    Ok(entries)
}

fn is_rom(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gb") || ext.eq_ignore_ascii_case("gbc"))
}

/// Les `max` dernières lettres de `text`, précédées de « .. » s'il a fallu couper.
fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let kept: String = text.chars().skip(count - (max - 2)).collect();
    format!("..{kept}")
}

/// Remplit une ligne du navigateur (toute la largeur, LINE_HEIGHT pixels de haut).
fn fill_line(screen: &mut [u8], y: usize, shade: u8) {
    screen[y * gui::WIDTH..(y + LINE_HEIGHT) * gui::WIDTH].fill(shade);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Un dossier temporaire propre à chaque test : un sous-dossier, deux ROM, et des
    /// fichiers que le navigateur doit ignorer.
    fn sample_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("retroemu-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("jeux")).unwrap();
        fs::create_dir_all(dir.join(".cache")).unwrap();
        for file in ["b.gb", "A.GBC", "notes.txt", ".cachee.gb"] {
            fs::write(dir.join(file), b"").unwrap();
        }
        dir
    }

    fn names(browser: &Browser) -> Vec<&str> {
        browser.entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    #[test]
    fn la_liste_est_triee_et_filtree() {
        let dir = sample_dir("liste");
        let browser = Browser::open(&dir).unwrap();
        assert_eq!(names(&browser), ["..", "jeux", "A.GBC", "b.gb"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn entree_ouvre_un_dossier_puis_remonte() {
        let dir = sample_dir("dossier");
        let mut browser = Browser::open(&dir).unwrap();
        browser.down(); // « jeux »
        assert_eq!(browser.enter().unwrap(), None);
        assert!(browser.dir.ends_with("jeux"));
        assert_eq!(names(&browser), [".."]);
        browser.enter().unwrap(); // « .. »
        assert_eq!(names(&browser), ["..", "jeux", "A.GBC", "b.gb"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn entree_sur_une_rom_la_renvoie() {
        let dir = sample_dir("rom");
        let mut browser = Browser::open(&dir).unwrap();
        browser.down();
        browser.down(); // « A.GBC »
        let path = browser.enter().unwrap().unwrap();
        assert!(path.ends_with("A.GBC"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn un_clic_sur_une_ligne_l_ouvre() {
        let dir = sample_dir("clic");
        let mut browser = Browser::open(&dir).unwrap();
        assert_eq!(browser.click(0).unwrap(), None); // la ligne du dossier
        assert_eq!(browser.click(9).unwrap(), None); // sous la dernière entrée
        let path = browser.click(4).unwrap().unwrap(); // ligne 4 = entrée 3 = « b.gb »
        assert!(path.ends_with("b.gb"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn la_liste_defile_avec_la_selection() {
        let dir = sample_dir("defilement");
        for i in 0..20 {
            fs::write(dir.join(format!("rom{i:02}.gb")), b"").unwrap();
        }
        let mut browser = Browser::open(&dir).unwrap(); // 24 entrées
        for _ in 0..30 {
            browser.down();
        }
        assert_eq!(browser.selected, 23); // bloqué sur la dernière
        assert_eq!(browser.scroll, 23 + 1 - VISIBLE); // elle est en bas de l'écran
        for _ in 0..30 {
            browser.up();
        }
        assert_eq!((browser.selected, browser.scroll), (0, 0));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn la_ligne_surlignee_est_foncee() {
        let dir = sample_dir("dessin");
        let mut browser = Browser::open(&dir).unwrap();
        browser.down();
        let mut screen = vec![0u8; gui::WIDTH * gui::HEIGHT];
        browser.draw(&mut screen);
        assert_eq!(screen[LINE_HEIGHT * gui::WIDTH], 0); // ligne 1 : « .. »
        assert_eq!(screen[2 * LINE_HEIGHT * gui::WIDTH], 3); // ligne 2 : « jeux », surlignée
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn un_chemin_trop_long_garde_sa_fin() {
        assert_eq!(tail("/a/b", 26), "/a/b");
        assert_eq!(tail("/users/dam/roms/blargg", 10), "..s/blargg");
    }
}
