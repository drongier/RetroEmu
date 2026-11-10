//! Les contrôleurs de banques (MBC) : la puce de la cartouche qui choisit quelle banque
//! de ROM est visible en 0x4000–0x7FFF, et quelle banque de RAM en 0xA000–0xBFFF.

/// Taille d'une banque de ROM : la moitié de la fenêtre 0x0000–0x7FFF.
const ROM_BANK_SIZE: usize = 0x4000;
/// Taille d'une banque de RAM : toute la fenêtre 0xA000–0xBFFF.
const RAM_BANK_SIZE: usize = 0x2000;

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
    /// Toute la RAM de la cartouche, pour l'écrire dans le fichier .sav. Vide s'il n'y en a pas.
    fn ram(&self) -> &[u8] {
        &[]
    }
    /// La même, modifiable, pour la remplir avec le contenu du .sav au lancement.
    fn ram_mut(&mut self) -> &mut [u8] {
        &mut []
    }
}

/// Cartouche sans MBC : 32 KiB de ROM directement visibles, pas de RAM.
pub struct RomOnly {
    rom: Vec<u8>,
}

impl RomOnly {
    pub fn new(rom: Vec<u8>) -> RomOnly {
        RomOnly { rom }
    }
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

/// Octet `offset` de la banque `bank` de la ROM. Une banque qui n'existe pas (au-delà de
/// la taille de la ROM) revient au début : le MBC ignore les bits de numéro en trop.
fn read_banked_rom(rom: &[u8], bank: usize, offset: usize) -> u8 {
    let banks = (rom.len() / ROM_BANK_SIZE).max(1);
    rom.get((bank % banks) * ROM_BANK_SIZE + offset).copied().unwrap_or(0xFF)
}

/// Position dans `ram` de l'adresse `addr` (0xA000–0xBFFF) de la banque `bank`.
/// Le modulo replie les petites RAM (2 Ko) et les banques qui n'existent pas.
fn banked_ram_index(ram: &[u8], bank: usize, addr: u16) -> usize {
    (bank * RAM_BANK_SIZE + (addr - 0xA000) as usize) % ram.len()
}

/// MBC1 : jusqu'à 2 Mo de ROM (125 banques utiles) et 32 Ko de RAM (4 banques).
pub struct Mbc1 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    /// 0x0000–0x1FFF : la RAM n'est accessible qu'après l'écriture de 0x0A.
    ram_enabled: bool,
    /// 0x2000–0x3FFF : les 5 bits bas du numéro de banque ROM. Jamais 0 (0 devient 1).
    bank_low: u8,
    /// 0x4000–0x5FFF : 2 bits, soit les bits 5-6 de la banque ROM, soit la banque de RAM.
    bank_high: u8,
    /// 0x6000–0x7FFF : en mode 1, `bank_high` s'applique aussi à 0x0000–0x3FFF et à la RAM.
    advanced_mode: bool,
}

impl Mbc1 {
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Mbc1 {
        Mbc1 {
            rom,
            ram: vec![0; ram_size],
            ram_enabled: false,
            bank_low: 1,
            bank_high: 0,
            advanced_mode: false,
        }
    }

    /// Position dans `ram` de l'adresse `addr` (0xA000–0xBFFF), ou `None` si la RAM est
    /// désactivée ou absente.
    fn ram_index(&self, addr: u16) -> Option<usize> {
        if !self.ram_enabled || self.ram.is_empty() {
            return None;
        }
        let bank = if self.advanced_mode { self.bank_high as usize } else { 0 };
        Some(banked_ram_index(&self.ram, bank, addr))
    }
}

impl Mbc for Mbc1 {
    fn read_rom(&self, addr: u16) -> u8 {
        let bank = match addr {
            // Mode 0 : toujours la banque 0. Mode 1 : 0x00, 0x20, 0x40 ou 0x60.
            0x0000..=0x3FFF if self.advanced_mode => (self.bank_high as usize) << 5,
            0x0000..=0x3FFF => 0,
            // Les 2 bits hauts collés devant les 5 bits bas : un numéro sur 7 bits.
            _ => (self.bank_high as usize) << 5 | self.bank_low as usize,
        };
        read_banked_rom(&self.rom, bank, (addr & 0x3FFF) as usize)
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = value & 0x0F == 0x0A,
            0x2000..=0x3FFF => {
                // La règle du MBC1 : 0 devient 1, car la banque 0 est déjà en 0x0000–0x3FFF.
                self.bank_low = match value & 0x1F {
                    0 => 1,
                    bank => bank,
                };
            }
            0x4000..=0x5FFF => self.bank_high = value & 0x03,
            _ => self.advanced_mode = value & 0x01 != 0,
        }
    }

    fn read_ram(&self, addr: u16) -> u8 {
        self.ram_index(addr).map_or(0xFF, |i| self.ram[i])
    }

    fn write_ram(&mut self, addr: u16, value: u8) {
        if let Some(i) = self.ram_index(addr) {
            self.ram[i] = value;
        }
    }

    fn ram(&self) -> &[u8] {
        &self.ram
    }

    fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }
}

/// Le MBC2 a sa RAM dans la puce : 512 cases de 4 bits.
const MBC2_RAM_SIZE: usize = 512;

/// MBC2 : jusqu'à 256 Ko de ROM (16 banques) et 512 × 4 bits de RAM intégrée.
pub struct Mbc2 {
    rom: Vec<u8>,
    /// Une case par octet, seuls les 4 bits bas sont utilisés.
    ram: [u8; MBC2_RAM_SIZE],
    ram_enabled: bool,
    /// Numéro de banque ROM sur 4 bits. Jamais 0 (0 devient 1, comme sur le MBC1).
    bank: u8,
}

impl Mbc2 {
    pub fn new(rom: Vec<u8>) -> Mbc2 {
        Mbc2 { rom, ram: [0; MBC2_RAM_SIZE], ram_enabled: false, bank: 1 }
    }

    /// Position de `addr` dans la RAM : les 512 cases se répètent sur toute la fenêtre
    /// 0xA000–0xBFFF, seuls les 9 bits bas de l'adresse comptent.
    fn ram_index(addr: u16) -> usize {
        (addr as usize - 0xA000) % MBC2_RAM_SIZE
    }
}

impl Mbc for Mbc2 {
    fn read_rom(&self, addr: u16) -> u8 {
        let bank = if addr < 0x4000 { 0 } else { self.bank as usize };
        read_banked_rom(&self.rom, bank, (addr & 0x3FFF) as usize)
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        // Un seul registre, en 0x0000–0x3FFF ; c'est le bit 8 de l'adresse qui dit lequel.
        if addr >= 0x4000 {
            return;
        }
        if addr & 0x0100 == 0 {
            self.ram_enabled = value & 0x0F == 0x0A;
        } else {
            self.bank = match value & 0x0F {
                0 => 1,
                bank => bank,
            };
        }
    }

    fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled {
            return 0xFF;
        }
        // Les 4 bits hauts n'existent pas : ils se lisent à 1.
        0xF0 | self.ram[Mbc2::ram_index(addr)]
    }

    fn write_ram(&mut self, addr: u16, value: u8) {
        if self.ram_enabled {
            self.ram[Mbc2::ram_index(addr)] = value & 0x0F;
        }
    }

    fn ram(&self) -> &[u8] {
        &self.ram
    }

    fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }
}

/// MBC5 : jusqu'à 8 Mo de ROM (512 banques) et 128 Ko de RAM (16 banques).
pub struct Mbc5 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    ram_enabled: bool,
    /// Numéro de banque ROM sur 9 bits : 8 bits en 0x2000–0x2FFF, le bit 8 en 0x3000–0x3FFF.
    /// La banque 0 est permise : pas de règle « 0 devient 1 ».
    rom_bank: u16,
    /// Banque de RAM sur 4 bits, en 0x4000–0x5FFF.
    ram_bank: u8,
}

impl Mbc5 {
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Mbc5 {
        Mbc5 { rom, ram: vec![0; ram_size], ram_enabled: false, rom_bank: 1, ram_bank: 0 }
    }

    fn ram_index(&self, addr: u16) -> Option<usize> {
        if !self.ram_enabled || self.ram.is_empty() {
            return None;
        }
        Some(banked_ram_index(&self.ram, self.ram_bank as usize, addr))
    }
}

impl Mbc for Mbc5 {
    fn read_rom(&self, addr: u16) -> u8 {
        let bank = if addr < 0x4000 { 0 } else { self.rom_bank as usize };
        read_banked_rom(&self.rom, bank, (addr & 0x3FFF) as usize)
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = value & 0x0F == 0x0A,
            // On remplace les 8 bits bas et on garde le bit 8, ou l'inverse.
            0x2000..=0x2FFF => self.rom_bank = (self.rom_bank & 0x100) | value as u16,
            0x3000..=0x3FFF => self.rom_bank = (self.rom_bank & 0x0FF) | (value as u16 & 1) << 8,
            0x4000..=0x5FFF => self.ram_bank = value & 0x0F,
            _ => {} // 0x6000–0x7FFF : rien sur le MBC5
        }
    }

    fn read_ram(&self, addr: u16) -> u8 {
        self.ram_index(addr).map_or(0xFF, |i| self.ram[i])
    }

    fn write_ram(&mut self, addr: u16, value: u8) {
        if let Some(i) = self.ram_index(addr) {
            self.ram[i] = value;
        }
    }

    fn ram(&self) -> &[u8] {
        &self.ram
    }

    fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une ROM de `banks` banques : le premier octet de chaque banque est son numéro.
    fn numbered_rom(banks: usize) -> Vec<u8> {
        let mut rom = vec![0; banks * ROM_BANK_SIZE];
        for bank in 0..banks {
            rom[bank * ROM_BANK_SIZE] = bank as u8;
        }
        rom
    }

    #[test]
    fn rom_only_lit_la_rom_et_ignore_les_ecritures() {
        let mut mbc = RomOnly::new(numbered_rom(2));
        mbc.write_rom(0x2000, 0x05);
        assert_eq!(mbc.read_rom(0x4000), 1);
        assert_eq!(mbc.read_ram(0xA000), 0xFF);
    }

    #[test]
    fn mbc1_change_de_banque_en_4000() {
        let mut mbc = Mbc1::new(numbered_rom(32), 0);
        assert_eq!(mbc.read_rom(0x0000), 0); // la banque 0, fixe
        assert_eq!(mbc.read_rom(0x4000), 1); // au démarrage : la banque 1
        mbc.write_rom(0x2000, 5);
        assert_eq!(mbc.read_rom(0x4000), 5);
        mbc.write_rom(0x3FFF, 7); // toute la plage 2000–3FFF est le même registre
        assert_eq!(mbc.read_rom(0x4000), 7);
    }

    #[test]
    fn mbc1_zero_devient_un() {
        let mut mbc = Mbc1::new(numbered_rom(64), 0); // 1 Mo
        mbc.write_rom(0x2000, 0);
        assert_eq!(mbc.read_rom(0x4000), 1);
        mbc.write_rom(0x4000, 1); // bits 5-6 = 01 : banque 0x20…
        assert_eq!(mbc.read_rom(0x4000), 0x21); // …qui donne 0x21
        mbc.write_rom(0x2000, 3);
        assert_eq!(mbc.read_rom(0x4000), 0x23);
    }

    #[test]
    fn mbc1_ignore_les_bits_en_trop() {
        let mut mbc = Mbc1::new(numbered_rom(4), 0); // 64 Ko : banques 0 à 3
        mbc.write_rom(0x2000, 6); // 6 = 0b110 → 0b10 = 2
        assert_eq!(mbc.read_rom(0x4000), 2);
    }

    #[test]
    fn mbc1_mode_1_change_aussi_le_debut_de_la_rom() {
        let mut mbc = Mbc1::new(numbered_rom(64), 0);
        mbc.write_rom(0x4000, 1);
        assert_eq!(mbc.read_rom(0x0000), 0); // mode 0 : toujours la banque 0
        mbc.write_rom(0x6000, 1);
        assert_eq!(mbc.read_rom(0x0000), 0x20); // mode 1 : la banque 0x20
    }

    #[test]
    fn mbc1_ram_desactivee_par_defaut() {
        let mut mbc = Mbc1::new(numbered_rom(4), 0x2000);
        mbc.write_ram(0xA000, 0x42);
        assert_eq!(mbc.read_ram(0xA000), 0xFF); // lue 0xFF, écriture perdue
        mbc.write_rom(0x0000, 0x0A);
        assert_eq!(mbc.read_ram(0xA000), 0x00); // l'écriture d'avant n'a pas eu lieu
        mbc.write_ram(0xA000, 0x42);
        assert_eq!(mbc.read_ram(0xA000), 0x42);
        mbc.write_rom(0x0000, 0x00); // on la referme
        assert_eq!(mbc.read_ram(0xA000), 0xFF);
    }

    #[test]
    fn mbc1_banques_de_ram_en_mode_1() {
        let mut mbc = Mbc1::new(numbered_rom(4), 4 * RAM_BANK_SIZE); // 32 Ko
        mbc.write_rom(0x0000, 0x0A);
        mbc.write_ram(0xA000, 0x11); // banque 0
        mbc.write_rom(0x4000, 2);
        mbc.write_ram(0xA000, 0x22); // mode 0 : toujours la banque 0
        assert_eq!(mbc.read_ram(0xA000), 0x22);
        mbc.write_rom(0x6000, 1); // mode 1 : banque 2
        mbc.write_ram(0xA000, 0x33);
        assert_eq!(mbc.read_ram(0xA000), 0x33);
        mbc.write_rom(0x4000, 0);
        assert_eq!(mbc.read_ram(0xA000), 0x22); // la banque 0 a gardé sa valeur
    }

    #[test]
    fn mbc2_le_bit_8_choisit_le_registre() {
        let mut mbc = Mbc2::new(numbered_rom(16));
        mbc.write_rom(0x2000, 5); // bit 8 = 0 : c'est le registre RAM, pas la banque
        assert_eq!(mbc.read_rom(0x4000), 1);
        mbc.write_rom(0x2100, 5); // bit 8 = 1 : la banque
        assert_eq!(mbc.read_rom(0x4000), 5);
        mbc.write_rom(0x0100, 0); // bit 8 = 1 aussi : 0 devient 1
        assert_eq!(mbc.read_rom(0x4000), 1);
        mbc.write_rom(0x4100, 7); // hors de 0000–3FFF : ignoré
        assert_eq!(mbc.read_rom(0x4000), 1);
    }

    #[test]
    fn mbc2_ram_de_4_bits_repetee() {
        let mut mbc = Mbc2::new(numbered_rom(2));
        mbc.write_ram(0xA000, 0x0C);
        assert_eq!(mbc.read_ram(0xA000), 0xFF); // désactivée
        mbc.write_rom(0x0000, 0x0A); // bit 8 = 0 : active la RAM
        mbc.write_ram(0xA000, 0xAB);
        assert_eq!(mbc.read_ram(0xA000), 0xFB); // seuls les 4 bits bas sont gardés
        assert_eq!(mbc.read_ram(0xA200), 0xFB); // 512 octets plus loin : la même case
        assert_eq!(mbc.read_ram(0xBE00), 0xFB);
    }

    /// Une ROM de `banks` banques où les 2 premiers octets de chaque banque forment son
    /// numéro sur 16 bits : il en faut plus de 256 pour tester le bit 8.
    fn numbered_rom_16(banks: usize) -> Vec<u8> {
        let mut rom = vec![0; banks * ROM_BANK_SIZE];
        for bank in 0..banks {
            rom[bank * ROM_BANK_SIZE..bank * ROM_BANK_SIZE + 2].copy_from_slice(&(bank as u16).to_le_bytes());
        }
        rom
    }

    fn rom_bank_number(mbc: &impl Mbc) -> u16 {
        u16::from_le_bytes([mbc.read_rom(0x4000), mbc.read_rom(0x4001)])
    }

    #[test]
    fn mbc5_banque_sur_9_bits() {
        let mut mbc = Mbc5::new(numbered_rom_16(512), 0); // 8 Mo
        assert_eq!(rom_bank_number(&mbc), 1);
        mbc.write_rom(0x2000, 0x34);
        mbc.write_rom(0x3000, 1); // bit 8
        assert_eq!(rom_bank_number(&mbc), 0x134);
        mbc.write_rom(0x2000, 0x05); // les 8 bits bas changent, le bit 8 reste
        assert_eq!(rom_bank_number(&mbc), 0x105);
        mbc.write_rom(0x3000, 0);
        assert_eq!(rom_bank_number(&mbc), 0x005);
    }

    #[test]
    fn mbc5_la_banque_0_est_permise() {
        let mut mbc = Mbc5::new(numbered_rom(4), 0);
        mbc.write_rom(0x2000, 0);
        assert_eq!(mbc.read_rom(0x4000), 0); // pas de « 0 devient 1 »
    }

    #[test]
    fn mbc5_seize_banques_de_ram() {
        let mut mbc = Mbc5::new(numbered_rom(4), 16 * RAM_BANK_SIZE); // 128 Ko
        mbc.write_rom(0x0000, 0x0A);
        for bank in 0..16 {
            mbc.write_rom(0x4000, bank);
            mbc.write_ram(0xA000, bank * 10);
        }
        mbc.write_rom(0x4000, 7);
        assert_eq!(mbc.read_ram(0xA000), 70);
        mbc.write_rom(0x4000, 15);
        assert_eq!(mbc.read_ram(0xA000), 150);
    }
}
