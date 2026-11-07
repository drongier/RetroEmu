//! Debugger : arrête le temps et regarde à l'intérieur de la console.
//!
//! Il reçoit des commandes texte (tapées dans le terminal) et agit sur la GameBoy :
//! exécuter une instruction, une image, une seconde, poser des points d'arrêt, lire la mémoire.
//! Il ne lit et n'écrit jamais lui-même dans le terminal en attendant : c'est `main.rs` qui
//! lui transmet les lignes, pour que la fenêtre reste fluide.

use anyhow::{Result, bail};

use crate::cpu::disassemble;
use crate::gameboy::GameBoy;

/// Images dans une seconde d'émulation (59,7 en vrai, arrondi).
const FRAMES_PER_SECOND: u32 = 60;

const HELP: &str = "\
Commandes :
  s [n]        exécute 1 (ou n) instruction(s)
  f            exécute une image
  sec          exécute une seconde (60 images)
  c            continue jusqu'au prochain point d'arrêt
  p            met en pause
  r            affiche les registres et la prochaine instruction
  b <adr>      pose un point d'arrêt (adresse en hexadécimal : b 0150)
  d <adr>      retire un point d'arrêt
  bl           liste les points d'arrêt
  x <adr> [n]  affiche n octets de mémoire (16 par défaut)
  q            quitte
  Entrée       répète la dernière commande";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Step(u32),
    Frame,
    Second,
    Continue,
    Pause,
    Registers,
    Break(u16),
    Delete(u16),
    ListBreaks,
    Memory(u16, u16),
    Help,
    Quit,
}

impl Command {
    /// Transforme une ligne tapée par l'utilisateur en commande.
    pub fn parse(line: &str) -> Result<Command> {
        let words: Vec<&str> = line.split_whitespace().collect();
        let command = match words.as_slice() {
            ["s"] => Command::Step(1),
            ["s", n] => Command::Step(parse_count(n)?),
            ["f"] => Command::Frame,
            ["sec"] => Command::Second,
            ["c"] => Command::Continue,
            ["p"] => Command::Pause,
            ["r"] => Command::Registers,
            ["b", addr] => Command::Break(parse_addr(addr)?),
            ["d", addr] => Command::Delete(parse_addr(addr)?),
            ["bl"] => Command::ListBreaks,
            ["x", addr] => Command::Memory(parse_addr(addr)?, 16),
            ["x", addr, n] => Command::Memory(parse_addr(addr)?, parse_count(n)? as u16),
            ["h"] | ["help"] => Command::Help,
            ["q"] => Command::Quit,
            _ => bail!("commande inconnue : « {line} » (h pour l'aide)"),
        };
        Ok(command)
    }
}

/// Une adresse en hexadécimal, avec ou sans « 0x ».
fn parse_addr(text: &str) -> Result<u16> {
    let digits = text.trim_start_matches("0x");
    u16::from_str_radix(digits, 16).map_err(|_| anyhow::anyhow!("adresse invalide : {text}"))
}

/// Un nombre en décimal.
fn parse_count(text: &str) -> Result<u32> {
    text.parse().map_err(|_| anyhow::anyhow!("nombre invalide : {text}"))
}

pub struct Debugger {
    /// En pause, l'émulation n'avance que sur commande.
    paused: bool,
    breakpoints: Vec<u16>,
    /// Rejouée quand l'utilisateur tape juste Entrée.
    last_command: Option<Command>,
}

impl Debugger {
    /// Le debugger démarre en pause : rien ne s'exécute avant la première commande.
    pub fn new() -> Debugger {
        Debugger { paused: true, breakpoints: Vec::new(), last_command: None }
    }

    /// Affiche l'aide et l'état de départ.
    pub fn start(&self, gb: &GameBoy) {
        println!("{HELP}");
        print_state(gb);
    }

    /// Traite une ligne tapée dans le terminal. Renvoie `false` si l'utilisateur veut quitter.
    pub fn handle_line(&mut self, gb: &mut GameBoy, line: &str) -> bool {
        let command = if line.trim().is_empty() {
            match self.last_command {
                Some(command) => command,
                None => return true,
            }
        } else {
            match Command::parse(line) {
                Ok(command) => command,
                Err(err) => {
                    println!("{err}");
                    return true;
                }
            }
        };
        self.last_command = Some(command);

        if let Err(err) = self.execute(gb, command) {
            // Une erreur du CPU (opcode illégal…) ne ferme pas l'émulateur : on peut regarder.
            println!("erreur : {err}");
            self.paused = true;
            print_state(gb);
        }
        command != Command::Quit
    }

    /// À appeler une fois par image depuis la boucle principale : fait tourner l'émulation
    /// si on n'est pas en pause, et s'arrête sur les points d'arrêt.
    pub fn update(&mut self, gb: &mut GameBoy) -> Result<()> {
        if self.paused {
            return Ok(());
        }
        if self.run_frame(gb)? {
            self.paused = true;
            println!("point d'arrêt atteint");
            print_state(gb);
        }
        Ok(())
    }

    fn execute(&mut self, gb: &mut GameBoy, command: Command) -> Result<()> {
        match command {
            Command::Step(count) => {
                self.paused = true;
                for _ in 0..count {
                    gb.step()?;
                }
                print_state(gb);
            }
            Command::Frame | Command::Second => {
                self.paused = true;
                let frames = if command == Command::Frame { 1 } else { FRAMES_PER_SECOND };
                for _ in 0..frames {
                    if self.run_frame(gb)? {
                        println!("point d'arrêt atteint");
                        break;
                    }
                }
                print_state(gb);
            }
            Command::Continue => self.paused = false,
            Command::Pause => {
                self.paused = true;
                print_state(gb);
            }
            Command::Registers => print_state(gb),
            Command::Break(addr) => {
                if !self.breakpoints.contains(&addr) {
                    self.breakpoints.push(addr);
                }
                println!("point d'arrêt en {addr:04X}");
            }
            Command::Delete(addr) => {
                self.breakpoints.retain(|&b| b != addr);
                println!("point d'arrêt {addr:04X} retiré");
            }
            Command::ListBreaks => {
                for addr in &self.breakpoints {
                    println!("  {addr:04X}");
                }
            }
            Command::Memory(addr, count) => print_memory(gb, addr, count),
            Command::Help => println!("{HELP}"),
            Command::Quit => {}
        }
        Ok(())
    }

    /// Exécute la fin de l'image en cours. Renvoie `true` si un point d'arrêt l'a interrompue.
    ///
    /// Si PC est déjà sur un point d'arrêt (on vient de s'y arrêter), on exécute d'abord
    /// cette instruction : sinon on resterait bloqué dessus pour toujours.
    fn run_frame(&self, gb: &mut GameBoy) -> Result<bool> {
        if self.breakpoints.contains(&gb.cpu().pc()) {
            gb.step()?;
        }
        gb.run_frame_until(|pc| self.breakpoints.contains(&pc))
    }
}

/// La prochaine instruction et les registres :
/// ```text
///   0150  F0 44     LDH A, (0xFF44)
///   A:01 F:B0 [Z-HC] B:00 C:13 … PC:0150 IME:0  LY:90
/// ```
fn print_state(gb: &GameBoy) {
    let bus = gb.bus();
    let pc = gb.cpu().pc();
    let (text, len) = disassemble(bus, pc);
    let bytes: Vec<String> =
        (0..len).map(|i| format!("{:02X}", bus.read8(pc.wrapping_add(i)))).collect();
    println!("  {pc:04X}  {:<9} {text}", bytes.join(" "));
    println!("  {}  LY:{:02X}", gb.cpu(), bus.read8(0xFF44));
}

/// Affiche `count` octets à partir de `addr`, 16 par ligne.
fn print_memory(gb: &GameBoy, addr: u16, count: u16) {
    for row in (0..count).step_by(16) {
        let start = addr.wrapping_add(row);
        let bytes: Vec<String> = (0..16.min(count - row))
            .map(|i| format!("{:02X}", gb.bus().read8(start.wrapping_add(i))))
            .collect();
        println!("  {start:04X}: {}", bytes.join(" "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::Cartridge;

    fn nop_gameboy() -> GameBoy {
        GameBoy::new(Cartridge::from_bytes(vec![0; 0x8000]).unwrap()) // que des NOP
    }

    #[test]
    fn comprend_les_commandes() {
        assert_eq!(Command::parse("s").unwrap(), Command::Step(1));
        assert_eq!(Command::parse("  s   5 ").unwrap(), Command::Step(5));
        assert_eq!(Command::parse("b 0150").unwrap(), Command::Break(0x0150));
        assert_eq!(Command::parse("b 0x0150").unwrap(), Command::Break(0x0150));
        assert_eq!(Command::parse("x ff40 4").unwrap(), Command::Memory(0xFF40, 4));
        assert!(Command::parse("s beaucoup").is_err());
        assert!(Command::parse("b zzzz").is_err());
        assert!(Command::parse("vole").is_err());
    }

    #[test]
    fn step_avance_d_une_instruction() {
        let mut gb = nop_gameboy();
        let mut debugger = Debugger::new();
        debugger.handle_line(&mut gb, "s 3");
        assert_eq!(gb.cpu().pc(), 0x0103);
        debugger.handle_line(&mut gb, ""); // Entrée : rejoue « s 3 »
        assert_eq!(gb.cpu().pc(), 0x0106);
    }

    #[test]
    fn continue_s_arrete_au_point_d_arret() {
        let mut gb = nop_gameboy();
        let mut debugger = Debugger::new();
        debugger.handle_line(&mut gb, "b 0110");
        debugger.handle_line(&mut gb, "c");
        debugger.update(&mut gb).unwrap();
        assert_eq!(gb.cpu().pc(), 0x0110);
        assert!(debugger.paused);

        // Repartir depuis le point d'arrêt ne doit pas rester bloqué dessus.
        debugger.handle_line(&mut gb, "f");
        assert_ne!(gb.cpu().pc(), 0x0110);
    }

    #[test]
    fn en_pause_rien_ne_bouge() {
        let mut gb = nop_gameboy();
        let mut debugger = Debugger::new();
        debugger.update(&mut gb).unwrap();
        assert_eq!(gb.cpu().pc(), 0x0100);
    }

    #[test]
    fn q_demande_de_quitter() {
        let mut gb = nop_gameboy();
        let mut debugger = Debugger::new();
        assert!(debugger.handle_line(&mut gb, "r"));
        assert!(!debugger.handle_line(&mut gb, "q"));
    }
}
