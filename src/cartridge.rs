//! Cartouche : lecture du fichier ROM, analyse du header et choix du MBC.

use anyhow::{Context, Result, bail};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::mbc::{Mbc, Mbc1, Mbc2, Mbc5, RomOnly};

/// Informations lues dans le header de la ROM (0x0134–0x014F).
#[derive(Debug, Clone)]
pub struct Header {
    pub title: String,
    pub cgb_flag: u8,
    pub cartridge_type: u8,
    pub rom_size_code: u8,
    pub ram_size_code: u8,
    pub header_checksum: u8,
    pub computed_checksum: u8,
}

impl Header {
    fn parse(rom: &[u8]) -> Result<Header> {
        if rom.len() < 0x150 {
            bail!("fichier trop petit ({} octets) : le header demande au moins 0x150 octets", rom.len());
        }

        let cgb_flag = rom[0x0143];
        // Sur les jeux CGB, l'octet 0x0143 n'appartient plus au titre.
        let title_end = if cgb_flag & 0x80 != 0 { 0x0143 } else { 0x0144 };
        let title = rom[0x0134..title_end]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '?' })
            .collect();

        let mut computed_checksum: u8 = 0;
        for &b in &rom[0x0134..=0x014C] {
            computed_checksum = computed_checksum.wrapping_sub(b).wrapping_sub(1);
        }

        Ok(Header {
            title,
            cgb_flag,
            cartridge_type: rom[0x0147],
            rom_size_code: rom[0x0148],
            ram_size_code: rom[0x0149],
            header_checksum: rom[0x014D],
            computed_checksum,
        })
    }

    /// Taille de la ROM annoncée par le header : 32 KiB × 2^code.
    pub fn rom_size(&self) -> usize {
        (32 * 1024) << self.rom_size_code
    }

    /// Taille de la RAM externe annoncée par le header.
    pub fn ram_size(&self) -> usize {
        match self.ram_size_code {
            0x02 => 8 * 1024,
            0x03 => 32 * 1024,
            0x04 => 128 * 1024,
            0x05 => 64 * 1024,
            _ => 0,
        }
    }

    pub fn checksum_ok(&self) -> bool {
        self.header_checksum == self.computed_checksum
    }

    /// Le jeu sait-il utiliser les fonctions de la CGB ? (0x80 = compatible, 0xC0 = CGB uniquement)
    /// La cartouche a-t-elle une pile pour garder sa RAM console éteinte ?
    pub fn has_battery(&self) -> bool {
        matches!(self.cartridge_type, 0x03 | 0x06 | 0x09 | 0x0D | 0x0F | 0x10 | 0x13 | 0x1B | 0x1E | 0x22 | 0xFF)
    }

    pub fn supports_cgb(&self) -> bool {
        self.cgb_flag & 0x80 != 0
    }

    pub fn cartridge_type_name(&self) -> &'static str {
        match self.cartridge_type {
            0x00 => "ROM ONLY",
            0x01 => "MBC1",
            0x02 => "MBC1+RAM",
            0x03 => "MBC1+RAM+BATTERY",
            0x05 => "MBC2",
            0x06 => "MBC2+BATTERY",
            0x0F => "MBC3+TIMER+BATTERY",
            0x10 => "MBC3+TIMER+RAM+BATTERY",
            0x11 => "MBC3",
            0x12 => "MBC3+RAM",
            0x13 => "MBC3+RAM+BATTERY",
            0x19 => "MBC5",
            0x1A => "MBC5+RAM",
            0x1B => "MBC5+RAM+BATTERY",
            0x1C => "MBC5+RUMBLE",
            0x1D => "MBC5+RUMBLE+RAM",
            0x1E => "MBC5+RUMBLE+RAM+BATTERY",
            _ => "inconnu",
        }
    }
}

pub struct Cartridge {
    pub header: Header,
    mbc: Box<dyn Mbc>,
    /// Le fichier .sav, à côté de la ROM, pour les cartouches à pile qui ont de la RAM.
    save_path: Option<PathBuf>,
}

impl Cartridge {
    pub fn load(path: &Path) -> Result<Cartridge> {
        let rom = std::fs::read(path)
            .with_context(|| format!("impossible de lire la ROM {}", path.display()))?;
        let mut cartridge = Cartridge::from_bytes(rom)?;
        if cartridge.header.has_battery() && !cartridge.mbc.ram().is_empty() {
            let save_path = path.with_extension("sav"); // zelda.gb → zelda.sav
            cartridge.load_save(&save_path)?;
            cartridge.save_path = Some(save_path);
        }
        Ok(cartridge)
    }

    /// Recopie le fichier .sav dans la RAM. Pas de fichier : c'est une première partie.
    /// Un fichier illisible est une erreur : continuer écraserait la sauvegarde à la fermeture.
    fn load_save(&mut self, path: &Path) -> Result<()> {
        let data = match std::fs::read(path) {
            Ok(data) => data,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| format!("impossible de lire la sauvegarde {}", path.display()));
            }
        };
        let ram = self.mbc.ram_mut();
        if data.len() != ram.len() {
            log::warn!("{} fait {} octets au lieu de {}", path.display(), data.len(), ram.len());
        }
        let size = data.len().min(ram.len());
        ram[..size].copy_from_slice(&data[..size]);
        Ok(())
    }

    /// Écrit la RAM dans le fichier .sav. Sans pile, il n'y a rien à faire.
    pub fn save(&self) -> Result<()> {
        if let Some(path) = &self.save_path {
            std::fs::write(path, self.mbc.ram())
                .with_context(|| format!("impossible d'écrire la sauvegarde {}", path.display()))?;
        }
        Ok(())
    }

    pub fn from_bytes(rom: Vec<u8>) -> Result<Cartridge> {
        let header = Header::parse(&rom)?;

        if !header.checksum_ok() {
            log::warn!(
                "checksum du header invalide (lu {:#04X}, calculé {:#04X}) : une vraie console refuserait de démarrer",
                header.header_checksum,
                header.computed_checksum
            );
        }

        let mbc: Box<dyn Mbc> = match header.cartridge_type {
            0x00 => Box::new(RomOnly::new(rom)),
            0x01..=0x03 => Box::new(Mbc1::new(rom, header.ram_size())), // MBC1, +RAM, +BATTERY
            0x05 | 0x06 => Box::new(Mbc2::new(rom)), // MBC2, +BATTERY : sa RAM est dans la puce
            // MBC5, +RAM, +BATTERY, et les mêmes avec vibreur (« rumble »), qu'on ignore.
            0x19..=0x1E => Box::new(Mbc5::new(rom, header.ram_size())),
            other => bail!(
                "cartouche \"{}\" de type {:#04X} ({}) pas encore supportée",
                header.title,
                other,
                header.cartridge_type_name()
            ),
        };

        Ok(Cartridge { header, mbc, save_path: None })
    }

    pub fn read_rom(&self, addr: u16) -> u8 {
        self.mbc.read_rom(addr)
    }

    pub fn write_rom(&mut self, addr: u16, value: u8) {
        self.mbc.write_rom(addr, value);
    }

    pub fn read_ram(&self, addr: u16) -> u8 {
        self.mbc.read_ram(addr)
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        self.mbc.write_ram(addr, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fabrique une ROM de 32 KiB avec un header valide.
    fn fake_rom(title: &str, cartridge_type: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0134..0x0134 + title.len()].copy_from_slice(title.as_bytes());
        rom[0x0147] = cartridge_type;
        let mut x: u8 = 0;
        for &b in &rom[0x0134..=0x014C] {
            x = x.wrapping_sub(b).wrapping_sub(1);
        }
        rom[0x014D] = x;
        rom
    }

    #[test]
    fn parse_le_header() {
        let cart = Cartridge::from_bytes(fake_rom("TETRIS", 0x00)).unwrap();
        assert_eq!(cart.header.title, "TETRIS");
        assert!(cart.header.checksum_ok());
        assert_eq!(cart.header.rom_size(), 32 * 1024);
        assert_eq!(cart.header.ram_size(), 0);
    }

    #[test]
    fn refuse_un_fichier_trop_petit() {
        assert!(Cartridge::from_bytes(vec![0; 0x100]).is_err());
    }

    #[test]
    fn refuse_un_mbc_non_supporte() {
        assert!(Cartridge::from_bytes(fake_rom("POKEMON", 0x13)).is_err()); // MBC3 : hors sujet
    }

    #[test]
    fn choisit_le_mbc1() {
        let mut rom = fake_rom("ZELDA", 0x03); // MBC1+RAM+BATTERY
        rom[0x0149] = 0x03; // 32 Ko de RAM
        rom.resize(0x10000, 0); // 64 Ko : 4 banques
        rom[0x8000] = 0x42; // premier octet de la banque 2
        let mut cart = Cartridge::from_bytes(rom).unwrap();
        cart.write_rom(0x2000, 2);
        assert_eq!(cart.read_rom(0x4000), 0x42);
    }

    /// Écrit une ROM MBC1+RAM+BATTERY (8 Ko de RAM) dans un dossier temporaire propre au test.
    fn battery_rom_file(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("retroemu-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut rom = fake_rom("ZELDA", 0x03);
        rom[0x0149] = 0x02; // 8 Ko de RAM
        let path = dir.join("zelda.gb");
        std::fs::write(&path, rom).unwrap();
        path
    }

    #[test]
    fn la_ram_survit_dans_le_fichier_sav() {
        let path = battery_rom_file("sav");
        let mut cart = Cartridge::load(&path).unwrap();
        cart.write_rom(0x0000, 0x0A); // active la RAM
        cart.write_ram(0xA123, 0x42);
        cart.save().unwrap();
        assert!(path.with_extension("sav").exists());

        let mut cart = Cartridge::load(&path).unwrap(); // « on rallume la console »
        cart.write_rom(0x0000, 0x0A);
        assert_eq!(cart.read_ram(0xA123), 0x42);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn pas_de_sav_sans_pile() {
        let path = battery_rom_file("sans-pile");
        let mut rom = std::fs::read(&path).unwrap();
        rom[0x0147] = 0x02; // MBC1+RAM, sans BATTERY
        std::fs::write(&path, rom).unwrap();
        Cartridge::load(&path).unwrap().save().unwrap();
        assert!(!path.with_extension("sav").exists());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn un_sav_trop_court_remplit_le_debut() {
        let path = battery_rom_file("court");
        std::fs::write(path.with_extension("sav"), [1, 2, 3]).unwrap();
        let mut cart = Cartridge::load(&path).unwrap();
        cart.write_rom(0x0000, 0x0A);
        assert_eq!([cart.read_ram(0xA000), cart.read_ram(0xA002), cart.read_ram(0xA003)], [1, 3, 0]);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
