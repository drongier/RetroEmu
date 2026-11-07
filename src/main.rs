mod bus;
mod cartridge;
mod cpu;
mod debugger;
mod gameboy;
mod joypad;
mod ppu;
mod timer;

use std::io::Write;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Texture;

use cartridge::Cartridge;
use debugger::Debugger;
use gameboy::{CYCLES_PER_FRAME, GameBoy};
use joypad::Button;

/// Taille de l'écran Game Boy, en pixels (SDL compte en u32, la PPU en usize).
const SCREEN_WIDTH: u32 = ppu::SCREEN_WIDTH as u32;
const SCREEN_HEIGHT: u32 = ppu::SCREEN_HEIGHT as u32;
/// Les 4 verts de l'écran DMG, de la teinte 0 (la plus claire) à la teinte 3.
const DMG_COLORS: [[u8; 3]; 4] = [[224, 248, 208], [136, 192, 112], [52, 104, 86], [8, 24, 32]];
/// Agrandissement de la fenêtre : 160×144 est minuscule sur un écran moderne.
const SCALE: u32 = 4;
/// Durée réelle d'une image : 70 224 cycles à 4 194 304 Hz ≈ 16,74 ms.
const FRAME_DURATION: Duration = Duration::from_nanos(CYCLES_PER_FRAME as u64 * 1_000_000_000 / 4_194_304);
/// En mode --headless, on abandonne après 2 minutes de temps émulé.
const HEADLESS_MAX_FRAMES: u32 = 120 * 60;

fn main() -> Result<()> {
    env_logger::init();

    // ① Quelle ROM, et quelles options ?
    let args: Vec<String> = std::env::args().skip(1).collect();
    let headless = args.iter().any(|arg| arg == "--headless");
    let debug = args.iter().any(|arg| arg == "--debug");
    let Some(rom_path) = args.iter().find(|arg| !arg.starts_with("--")) else {
        bail!("usage : retro_emu [--headless | --debug] <fichier.gb>");
    };

    // ② Charger la cartouche et afficher son header.
    let cartridge = Cartridge::load(Path::new(&rom_path))?;
    let header = &cartridge.header;
    println!("Titre      : {}", header.title);
    println!("Type       : {:#04X} ({})", header.cartridge_type, header.cartridge_type_name());
    println!("ROM        : {} Ko", header.rom_size() / 1024);
    println!("RAM        : {} Ko", header.ram_size() / 1024);
    println!("Game Boy Color : {}", if header.supports_cgb() { "oui" } else { "non" });
    println!("Checksum   : {}", if header.checksum_ok() { "OK" } else { "invalide" });
    let title = format!("RetroEmu - {}", header.title);
    let mut gb = GameBoy::new(cartridge);

    if headless {
        return run_headless(&mut gb);
    }

    // ③ Ouvrir la fenêtre. SDL renvoie ses erreurs sous forme de String, d'où les map_err.
    let sdl = sdl2::init().map_err(|e| anyhow!(e))?;
    let video = sdl.video().map_err(|e| anyhow!(e))?;
    let window = video
        .window(&title, SCREEN_WIDTH * SCALE, SCREEN_HEIGHT * SCALE)
        .position_centered()
        .build()?;
    let mut canvas = window.into_canvas().build()?;
    let mut events = sdl.event_pump().map_err(|e| anyhow!(e))?;
    // La texture : une image de 160 × 144 côté carte graphique, que SDL étire à la taille
    // de la fenêtre. « Streaming » : on la remplit à nouveau à chaque image.
    let texture_creator = canvas.texture_creator();
    let mut texture =
        texture_creator.create_texture_streaming(PixelFormatEnum::RGB24, SCREEN_WIDTH, SCREEN_HEIGHT)?;
    let mut pixels = vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT * 3) as usize];

    // En mode --debug : le debugger, et la boîte aux lettres où arrivent les lignes tapées.
    let mut debug_session = if debug {
        let debugger = Debugger::new();
        debugger.start(&gb);
        print_prompt();
        Some((debugger, spawn_command_reader()))
    } else {
        None
    };

    // ④ La boucle principale : une image par tour.
    'running: loop {
        let frame_start = Instant::now();

        for event in events.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => break 'running,
                // repeat: false ignore les répétitions automatiques d'une touche maintenue.
                Event::KeyDown { keycode: Some(key), repeat: false, .. } => {
                    if let Some(button) = button_for(key) {
                        gb.press(button);
                    }
                }
                Event::KeyUp { keycode: Some(key), .. } => {
                    if let Some(button) = button_for(key) {
                        gb.release(button);
                    }
                }
                _ => {}
            }
        }

        match &mut debug_session {
            None => gb.run_frame()?,
            Some((debugger, commands)) => {
                // On relève toutes les lignes arrivées depuis la dernière image, sans attendre.
                loop {
                    match commands.try_recv() {
                        Ok(line) => {
                            if !debugger.handle_line(&mut gb, &line) {
                                break 'running;
                            }
                            print_prompt();
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => break 'running, // Ctrl-D
                    }
                }
                debugger.update(&mut gb)?;
            }
        }
        print_serial(&mut gb, &mut Vec::new());

        draw_screen(gb.framebuffer(), &mut pixels, &mut texture)?;
        canvas.copy(&texture, None, None).map_err(|e| anyhow!(e))?; // None, None : toute la texture, toute la fenêtre
        canvas.present();

        // On dort le temps qui reste pour tenir ~59,7 images par seconde.
        let elapsed = frame_start.elapsed();
        if elapsed < FRAME_DURATION {
            std::thread::sleep(FRAME_DURATION - elapsed);
        }
    }

    Ok(())
}

/// Traduit les teintes de la PPU en couleurs RGB et les envoie dans la texture.
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
