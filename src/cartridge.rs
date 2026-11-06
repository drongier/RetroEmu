//! Cartouche : lecture du fichier ROM, analyse du header et MBC.

use anyhow::{Context, Result, bail};
use std::path::Path;

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

/// Ce que tout contrôleur de banques (MBC) doit savoir faire.
///
/// Le bus ne connaît que ce trait : il n'a pas besoin de savoir quel MBC
/// se cache derrière pour lui transmettre les accès.
pub trait Mbc {
    /// Lecture dans 0x0000–0x7FFF.
    fn read_rom(&self, addr: u16) -> u8;
    /// Écriture dans 0x0000–0x7FFF : ne modifie jamais la ROM, ce sont des commandes pour le MBC.
    fn write_rom(&mut self, addr: u16, value: u8);
    /// Lecture dans 0xA000–0xBFFF.
    fn read_ram(&self, addr: u16) -> u8;
    /// Écriture dans 0xA000–0xBFFF.
    fn write_ram(&mut self, addr: u16, value: u8);
}

/// Cartouche sans MBC : 32 KiB de ROM directement visibles, pas de RAM.
struct RomOnly {
    rom: Vec<u8>,
}

impl Mbc for RomOnly {
    fn read_rom(&self, addr: u16) -> u8 {
        self.rom.get(addr as usize).copied().unwrap_or(0xFF)
    }

    fn write_rom(&mut self, _addr: u16, _value: u8) {}

    fn read_ram(&self, _addr: u16) -> u8 {
        0xFF
    }

    fn write_ram(&mut self, _addr: u16, _value: u8) {}
}

pub struct Cartridge {
    pub header: Header,
    mbc: Box<dyn Mbc>,
}

impl Cartridge {
    pub fn load(path: &Path) -> Result<Cartridge> {
        let rom = std::fs::read(path)
            .with_context(|| format!("impossible de lire la ROM {}", path.display()))?;
        Cartridge::from_bytes(rom)
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
            0x00 => Box::new(RomOnly { rom }),
            // Un MBC1 sans RAM de 32 Ko n'a qu'une banque à proposer : il se comporte comme
            // une ROM ONLY. C'est le cas des tests Blargg. Le vrai MBC1 arrive à l'étape 10.
            0x01 if rom.len() == 0x8000 => Box::new(RomOnly { rom }),
            other => bail!(
                "cartouche \"{}\" de type {:#04X} ({}) pas encore supportée",
                header.title,
                other,
                header.cartridge_type_name()
            ),
        };

        Ok(Cartridge { header, mbc })
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
    fn rom_only_lit_la_rom_et_ignore_les_ecritures() {
        let mut rom = fake_rom("TEST", 0x00);
        rom[0x4000] = 0x42;
        let mut cart = Cartridge::from_bytes(rom).unwrap();
        cart.write_rom(0x4000, 0x99);
        assert_eq!(cart.read_rom(0x4000), 0x42);
        assert_eq!(cart.read_ram(0xA000), 0xFF);
    }

    #[test]
    fn refuse_un_fichier_trop_petit() {
        assert!(Cartridge::from_bytes(vec![0; 0x100]).is_err());
    }

    #[test]
    fn refuse_un_mbc_non_supporte() {
        assert!(Cartridge::from_bytes(fake_rom("ZELDA", 0x03)).is_err());
    }

    #[test]
    fn accepte_un_mbc1_de_32_ko() {
        let mut rom = fake_rom("BLARGG", 0x01);
        rom.resize(0x8000, 0);
        assert!(Cartridge::from_bytes(rom.clone()).is_ok());
        rom.resize(0x10000, 0); // 64 Ko : il faudrait changer de banque
        assert!(Cartridge::from_bytes(rom).is_err());
    }
}
