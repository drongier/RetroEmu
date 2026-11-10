mod browser;
mod bus;
mod cartridge;
mod cpu;
mod debugger;
mod font;
mod gameboy;
mod gui;
mod joypad;
mod ppu;
mod timer;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseButton;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Texture;

use browser::Browser;
use cartridge::Cartridge;
use debugger::Debugger;
use gameboy::{CYCLES_PER_FRAME, GameBoy};
use joypad::Button;

/// Taille de l'écran virtuel (le jeu + la barre), en pixels.
/// SDL compte en u32, la GUI en usize.
const SCREEN_WIDTH: u32 = gui::WIDTH as u32;
const SCREEN_HEIGHT: u32 = gui::HEIGHT as u32;
/// Les 4 verts de l'écran DMG, de la teinte 0 (la plus claire) à la teinte 3.
const DMG_COLORS: [[u8; 3]; 4] = [[224, 248, 208], [136, 192, 112], [52, 104, 86], [8, 24, 32]];
/// Agrandissement de la fenêtre : 160×160 est minuscule sur un écran moderne.
const SCALE: u32 = 4;
/// Durée réelle d'une image : 70 224 cycles à 4 194 304 Hz ≈ 16,74 ms.
const FRAME_DURATION: Duration = Duration::from_nanos(CYCLES_PER_FRAME as u64 * 1_000_000_000 / 4_194_304);
/// En mode --headless, on abandonne après 2 minutes de temps émulé.
const HEADLESS_MAX_FRAMES: u32 = 120 * 60;

/// L'état de l'application : la console, s'il y a une ROM, et ce que la GUI affiche.
struct App {
    gb: Option<GameBoy>,
    rom_name: Option<String>,
    paused: bool,
    /// Le navigateur de fichiers, quand il est ouvert : il remplace l'image du jeu.
    browser: Option<Browser>,
    /// Le dossier où le navigateur s'ouvre : celui de la dernière ROM chargée.
    last_dir: PathBuf,
}

impl App {
    fn new() -> App {
        let last_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        App { gb: None, rom_name: None, paused: false, browser: None, last_dir }
    }

    /// Remplace la partie en cours par la ROM `path`. En cas d'erreur (fichier illisible,
    /// pas une ROM…), rien ne change : la partie en cours continue.
    fn load(&mut self, path: &Path) -> Result<()> {
        let cartridge = Cartridge::load(path)?;
        print_header(&cartridge);
        self.rom_name = Some(cartridge.header.title.clone());
        self.gb = Some(GameBoy::new(cartridge));
        self.paused = false;
        self.browser = None;
        // Le dossier de la ROM, sauf pour un nom seul (« tetris.gb ») : parent() donne "".
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            self.last_dir = dir.to_path_buf();
        }
        Ok(())
    }

    /// Charge une ROM ; en cas d'échec, explique pourquoi dans le terminal.
    fn load_or_report(&mut self, path: &Path) {
        if let Err(error) = self.load(path) {
            eprintln!("Impossible de charger {} : {error:#}", path.display());
        }
    }

    /// Ouvre le navigateur, ou le referme s'il est déjà ouvert.
    fn toggle_browser(&mut self) {
        if self.browser.is_some() {
            self.browser = None;
            return;
        }
        match Browser::open(&self.last_dir) {
            Ok(browser) => self.browser = Some(browser),
            Err(error) => eprintln!("{error:#}"),
        }
    }

    /// Les touches et les clics quand le navigateur est ouvert : la console n'en reçoit aucun.
    fn handle_browser_event(&mut self, event: &Event) {
        let Some(browser) = self.browser.as_mut() else { return };
        let chosen = match event {
            // Les répétitions sont acceptées : maintenir ↓ fait défiler la liste.
            Event::KeyDown { keycode: Some(Keycode::Up), .. } => {
                browser.up();
                Ok(None)
            }
            Event::KeyDown { keycode: Some(Keycode::Down), .. } => {
                browser.down();
                Ok(None)
            }
            Event::KeyDown { keycode: Some(Keycode::Return), repeat: false, .. } => browser.enter(),
            Event::KeyDown { keycode: Some(Keycode::Escape | Keycode::L), repeat: false, .. } => {
                self.browser = None;
                return;
            }
            Event::MouseButtonDown { mouse_btn: MouseButton::Left, x, y, .. } => {
                let (x, y) = to_virtual(*x, *y);
                if y < ppu::SCREEN_HEIGHT {
                    browser.click(y / browser::LINE_HEIGHT)
                } else {
                    if gui::button_at(x, y) == Some(gui::BarButton::Load) {
                        self.browser = None;
                    }
                    return;
                }
            }
            _ => Ok(None),
        };
        match chosen {
            Ok(Some(path)) => self.load_or_report(&path), // referme le navigateur si ça marche
            Ok(None) => {}
            Err(error) => eprintln!("{error:#}"),
        }
    }

    /// Pause ⇄ lecture. Sans ROM, il n'y a rien à mettre en pause.
    fn toggle_pause(&mut self) {
        if self.gb.is_some() {
            self.paused = !self.paused;
        }
    }

    fn window_title(&self) -> String {
        match &self.rom_name {
            Some(name) => format!("RetroEmu - {name}"),
            None => "RetroEmu".to_string(),
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();

    // ① Quelle ROM, et quelles options ? Sans ROM, la fenêtre s'ouvre sur l'écran d'accueil.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let headless = args.iter().any(|arg| arg == "--headless");
    let debug = args.iter().any(|arg| arg == "--debug");
    let rom_path = args.iter().find(|arg| !arg.starts_with("--"));
    if rom_path.is_none() && (headless || debug) {
        bail!("usage : retro_emu [--headless | --debug] <fichier.gb>");
    }

    // ② Charger la cartouche, si on en a une.
    let mut app = App::new();
    if let Some(path) = rom_path {
        app.load(Path::new(path))?;
    }
    if headless {
        if let Some(gb) = app.gb.as_mut() {
            return run_headless(gb);
        }
    }

    // ③ Ouvrir la fenêtre. SDL renvoie ses erreurs sous forme de String, d'où les map_err.
    let sdl = sdl2::init().map_err(|e| anyhow!(e))?;
    let video = sdl.video().map_err(|e| anyhow!(e))?;
    let window = video
        .window(&app.window_title(), SCREEN_WIDTH * SCALE, SCREEN_HEIGHT * SCALE)
        .position_centered()
        .build()?;
    let mut canvas = window.into_canvas().build()?;
    let mut events = sdl.event_pump().map_err(|e| anyhow!(e))?;
    // La texture : l'écran virtuel côté carte graphique, que SDL étire à la taille de la
    // fenêtre. « Streaming » : on la remplit à nouveau à chaque image.
    let texture_creator = canvas.texture_creator();
    let mut texture =
        texture_creator.create_texture_streaming(PixelFormatEnum::RGB24, SCREEN_WIDTH, SCREEN_HEIGHT)?;
    // L'écran virtuel en teintes, puis sa version RGB pour la texture.
    let mut screen = vec![0u8; gui::WIDTH * gui::HEIGHT];
    let mut pixels = vec![0u8; screen.len() * 3];

    // En mode --debug : le debugger, et la boîte aux lettres où arrivent les lignes tapées.
    let mut debug_session = match (&app.gb, debug) {
        (Some(gb), true) => {
            let debugger = Debugger::new();
            debugger.start(gb);
            print_prompt();
            Some((debugger, spawn_command_reader()))
        }
        _ => None,
    };

    // ④ La boucle principale : une image par tour.
    // L'heure à laquelle l'image en cours doit être terminée.
    let mut deadline = Instant::now();
    'running: loop {
        for event in events.poll_iter() {
            // Fermer la fenêtre et glisser un fichier marchent même navigateur ouvert.
            match &event {
                Event::Quit { .. } => break 'running,
                Event::DropFile { filename, .. } => {
                    app.load_or_report(Path::new(filename));
                    continue;
                }
                _ => {}
            }
            if app.browser.is_some() {
                app.handle_browser_event(&event);
                continue;
            }
            match event {
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => break 'running,
                Event::KeyDown { keycode: Some(Keycode::P), repeat: false, .. } => app.toggle_pause(),
                Event::KeyDown { keycode: Some(Keycode::L), repeat: false, .. } => app.toggle_browser(),
                // repeat: false ignore les répétitions automatiques d'une touche maintenue.
                Event::KeyDown { keycode: Some(key), repeat: false, .. } => {
                    if let (Some(gb), Some(button)) = (app.gb.as_mut(), button_for(key)) {
                        gb.press(button);
                    }
                }
                Event::KeyUp { keycode: Some(key), .. } => {
                    if let (Some(gb), Some(button)) = (app.gb.as_mut(), button_for(key)) {
                        gb.release(button);
                    }
                }
                Event::MouseButtonDown { mouse_btn: MouseButton::Left, x, y, .. } => {
                    let (x, y) = to_virtual(x, y);
                    match gui::button_at(x, y) {
                        Some(gui::BarButton::Pause) => app.toggle_pause(),
                        Some(gui::BarButton::Load) => app.toggle_browser(),
                        None => {}
                    }
                }
                _ => {}
            }
        }
        // Une ROM a peut-être été chargée pendant ce tour : le titre suit.
        let title = app.window_title();
        if canvas.window().title() != title {
            canvas.window_mut().set_title(&title)?;
        }

        // La console n'avance que si une ROM est chargée, hors pause et navigateur fermé.
        let frozen = app.paused || app.browser.is_some();
        if let Some(gb) = app.gb.as_mut() {
            match &mut debug_session {
                None => {
                    if !frozen {
                        gb.run_frame()?;
                    }
                }
                Some((debugger, commands)) => {
                    // On relève toutes les lignes arrivées depuis la dernière image, sans attendre.
                    loop {
                        match commands.try_recv() {
                            Ok(line) => {
                                if !debugger.handle_line(gb, &line) {
                                    break 'running;
                                }
                                print_prompt();
                            }
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => break 'running, // Ctrl-D
                        }
                    }
                    if !frozen {
                        debugger.update(gb)?;
                    }
                }
            }
            print_serial(gb, &mut Vec::new());
        }

        let game = app.gb.as_ref().map(|gb| &gb.framebuffer()[..]);
        gui::compose(&mut screen, game, app.rom_name.as_deref(), app.paused);
        if let Some(browser) = &app.browser {
            browser.draw(&mut screen);
        }
        draw_screen(&screen, &mut pixels, &mut texture)?;
        canvas.copy(&texture, None, None).map_err(|e| anyhow!(e))?; // None, None : toute la texture, toute la fenêtre
        canvas.present();

        wait_for_next_frame(&mut deadline);
    }

    Ok(())
}

/// Position de la souris dans la fenêtre → pixel de l'écran virtuel.
fn to_virtual(x: i32, y: i32) -> (usize, usize) {
    // Les coordonnées d'un clic dans la fenêtre ne sont jamais négatives.
    ((x / SCALE as i32) as usize, (y / SCALE as i32) as usize)
}

/// Affiche le header de la cartouche dans le terminal.
fn print_header(cartridge: &Cartridge) {
    let header = &cartridge.header;
    println!("Titre      : {}", header.title);
    println!("Type       : {:#04X} ({})", header.cartridge_type, header.cartridge_type_name());
    println!("ROM        : {} Ko", header.rom_size() / 1024);
    println!("RAM        : {} Ko", header.ram_size() / 1024);
    println!("Game Boy Color : {}", if header.supports_cgb() { "oui" } else { "non" });
    println!("Checksum   : {}", if header.checksum_ok() { "OK" } else { "invalide" });
}

/// Dort jusqu'à l'échéance de l'image suivante, pour tenir ~59,7 images par seconde.
/// L'échéance avance toujours de FRAME_DURATION : si `sleep` dort un peu trop longtemps,
/// le tour suivant dort un peu moins, et le retard ne s'accumule pas.
fn wait_for_next_frame(deadline: &mut Instant) {
    *deadline += FRAME_DURATION;
    let now = Instant::now();
    if *deadline > now {
        std::thread::sleep(*deadline - now);
    } else {
        // Très en retard (fenêtre déplacée, machine occupée…) : on repart d'ici au lieu
        // d'enchaîner les images en accéléré pour rattraper.
        *deadline = now;
    }
}

/// Traduit les teintes de l'écran virtuel en couleurs RGB et les envoie dans la texture.
fn draw_screen(framebuffer: &[u8], pixels: &mut [u8], texture: &mut Texture) -> Result<()> {
    // chunks_exact_mut(3) découpe `pixels` en morceaux de 3 octets : un par pixel (R, G, B).
    for (rgb, &shade) in pixels.chunks_exact_mut(3).zip(framebuffer) {
        rgb.copy_from_slice(&DMG_COLORS[shade as usize]);
    }
    // Le dernier argument : combien d'octets fait une ligne (160 pixels × 3).
    texture.update(None, pixels, SCREEN_WIDTH as usize * 3)?;
    Ok(())
}

/// Mode sans fenêtre : fait tourner la ROM à pleine vitesse et affiche ce qu'elle envoie
/// sur le port série. S'arrête quand un test Blargg écrit « Passed » ou « Failed ».
fn run_headless(gb: &mut GameBoy) -> Result<()> {
    let mut output = Vec::new();
    for _ in 0..HEADLESS_MAX_FRAMES {
        gb.run_frame()?;
        print_serial(gb, &mut output);

        let text = String::from_utf8_lossy(&output);
        if text.contains("Passed") {
            println!();
            return Ok(());
        }
        if text.contains("Failed") {
            println!();
            bail!("le test a échoué");
        }
    }
    bail!("aucun résultat après {} secondes émulées", HEADLESS_MAX_FRAMES / 60)
}

/// Lance un thread qui lit le terminal ligne par ligne et dépose chaque ligne dans un canal.
/// Lire le terminal bloque jusqu'à Entrée : c'est ce thread qui attend, jamais la fenêtre.
fn spawn_command_reader() -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break; // le thread principal est parti : plus personne pour lire
            }
        }
    });
    receiver
}

fn print_prompt() {
    print!("(debug) ");
    std::io::stdout().flush().ok();
}

/// Affiche dans le terminal ce que la ROM vient d'envoyer sur le port série,
/// et l'ajoute à `output`.
fn print_serial(gb: &mut GameBoy, output: &mut Vec<u8>) {
    let bytes = gb.take_serial_output();
    if !bytes.is_empty() {
        print!("{}", String::from_utf8_lossy(&bytes));
        std::io::stdout().flush().ok(); // sans retour à la ligne, print! attend avant d'afficher
        output.extend(bytes);
    }
}

/// Touche du clavier → bouton de la Game Boy.
fn button_for(key: Keycode) -> Option<Button> {
    match key {
        Keycode::Right => Some(Button::Right),
        Keycode::Left => Some(Button::Left),
        Keycode::Up => Some(Button::Up),
        Keycode::Down => Some(Button::Down),
        Keycode::X => Some(Button::A),
        Keycode::Z => Some(Button::B),
        Keycode::Return => Some(Button::Start),
        Keycode::Backspace => Some(Button::Select),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_retard_de_sleep_ne_s_accumule_pas() {
        const FRAMES: u32 = 30;
        let start = Instant::now();
        let mut deadline = start;
        for _ in 0..FRAMES {
            wait_for_next_frame(&mut deadline);
        }
        let elapsed = start.elapsed();
        // Au plus un seul dépassement de sleep (le dernier), pas 30 mis bout à bout.
        assert!(elapsed >= FRAME_DURATION * FRAMES);
        assert!(elapsed < FRAME_DURATION * FRAMES + Duration::from_millis(10), "{elapsed:?}");
    }

    #[test]
    fn une_rom_illisible_ne_change_rien() {
        let mut app = App::new();
        assert!(app.load(Path::new("ce/fichier/n/existe/pas.gb")).is_err());
        assert!(app.gb.is_none());
        assert_eq!(app.window_title(), "RetroEmu");
    }

    #[test]
    fn pas_de_pause_sans_rom() {
        let mut app = App::new();
        app.toggle_pause();
        assert!(!app.paused);
    }

    #[test]
    fn charger_une_rom_relance_la_lecture() {
        let mut app = App::new();
        app.load(Path::new("test-roms/acid2/dmg-acid2.gb")).unwrap();
        app.toggle_pause();
        assert!(app.paused);
        app.load(Path::new("test-roms/acid2/dmg-acid2.gb")).unwrap();
        assert!(!app.paused);
        assert_eq!(app.window_title(), "RetroEmu - DMG-ACID2");
    }

    #[test]
    fn charger_une_rom_ferme_le_navigateur_et_retient_son_dossier() {
        let mut app = App::new();
        app.toggle_browser();
        assert!(app.browser.is_some());
        app.load(Path::new("test-roms/acid2/dmg-acid2.gb")).unwrap();
        assert!(app.browser.is_none());
        assert_eq!(app.last_dir, Path::new("test-roms/acid2"));
    }
}
