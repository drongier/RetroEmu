mod bus;
mod cartridge;
mod cpu;
mod gameboy;
mod joypad;
mod ppu;
mod timer;

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;

use cartridge::Cartridge;
use gameboy::{CYCLES_PER_FRAME, GameBoy};
use joypad::Button;

/// Taille de l'écran Game Boy, en pixels.
const SCREEN_WIDTH: u32 = 160;
const SCREEN_HEIGHT: u32 = 144;
/// Agrandissement de la fenêtre : 160×144 est minuscule sur un écran moderne.
const SCALE: u32 = 4;
/// Durée réelle d'une image : 70 224 cycles à 4 194 304 Hz ≈ 16,74 ms.
const FRAME_DURATION: Duration = Duration::from_nanos(CYCLES_PER_FRAME as u64 * 1_000_000_000 / 4_194_304);

fn main() -> Result<()> {
    env_logger::init();

    // ① Quelle ROM ?
    let Some(rom_path) = std::env::args().nth(1) else {
        bail!("usage : retro_emu <fichier.gb>");
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

    // ③ Ouvrir la fenêtre. SDL renvoie ses erreurs sous forme de String, d'où les map_err.
    let sdl = sdl2::init().map_err(|e| anyhow!(e))?;
    let video = sdl.video().map_err(|e| anyhow!(e))?;
    let window = video
        .window(&title, SCREEN_WIDTH * SCALE, SCREEN_HEIGHT * SCALE)
        .position_centered()
        .build()?;
    let mut canvas = window.into_canvas().build()?;
    let mut events = sdl.event_pump().map_err(|e| anyhow!(e))?;

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

        gb.run_frame()?;

        // La PPU ne dessine pas encore : on affiche un écran uni (étape 7).
        canvas.set_draw_color(Color::RGB(155, 188, 15));
        canvas.clear();
        canvas.present();

        // On dort le temps qui reste pour tenir ~59,7 images par seconde.
        let elapsed = frame_start.elapsed();
        if elapsed < FRAME_DURATION {
            std::thread::sleep(FRAME_DURATION - elapsed);
        }
    }

    Ok(())
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
