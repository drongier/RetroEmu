//! Timer : registres DIV, TIMA, TMA et TAC (0xFF04–0xFF07).
//!
//! Comme sur le circuit, TIMA est branché sur un bit du compteur interne choisi par TAC,
//! combiné (ET) avec l'interrupteur de TAC. TIMA avance quand ce signal passe de 1 à 0
//! (front descendant). Ce modèle reproduit tout seul les bizarreries du matériel : remettre
//! DIV à zéro ou éteindre le timer peut faire avancer TIMA.
//!
//! Simplification : au débordement, TIMA reçoit TMA tout de suite. Sur la vraie console,
//! il vaut 0x00 pendant 4 T-cycles avant le rechargement.

/// TAC bit 2 : le timer est en marche.
const TAC_ENABLE: u8 = 0x04;

#[derive(Default)]
pub struct Timer {
    /// Compteur interne de 16 bits, +1 à chaque T-cycle. DIV en est l'octet haut.
    counter: u16,
    tima: u8,
    tma: u8,
    tac: u8,
    /// TIMA a débordé : l'interruption Timer attend d'être transmise au Bus.
    interrupt: bool,
}

impl Timer {
    pub fn new() -> Timer {
        Timer::default()
    }

    /// Avance de `cycles` T-cycles, 4 par 4 pour ne rater aucun front.
    /// Renvoie `true` si le timer demande une interruption.
    pub fn tick(&mut self, cycles: u32) -> bool {
        debug_assert!(cycles % 4 == 0, "le CPU avance toujours par M-cycles de 4 T-cycles");
        for _ in 0..cycles / 4 {
            let before = self.signal();
            self.counter = self.counter.wrapping_add(4);
            self.detect_fall(before);
        }
        std::mem::take(&mut self.interrupt)
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF04 => (self.counter >> 8) as u8,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac | 0xF8, // les 5 bits hauts n'existent pas et se lisent à 1
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF04 => {
                // Écrire n'importe quoi dans DIV remet le compteur à zéro.
                let before = self.signal();
                self.counter = 0;
                self.detect_fall(before);
            }
            0xFF05 => self.tima = value,
            0xFF06 => self.tma = value,
            0xFF07 => {
                let before = self.signal();
                self.tac = value & 0x07;
                self.detect_fall(before);
            }
            _ => {}
        }
    }

    /// Le fil qui arrive à TIMA : le bit du compteur choisi par TAC, ET l'interrupteur.
    fn signal(&self) -> bool {
        let bit = match self.tac & 0x03 {
            0 => 9, //   4 096 Hz : TIMA +1 tous les 1 024 T-cycles
            1 => 3, // 262 144 Hz : tous les 16
            2 => 5, //  65 536 Hz : tous les 64
            3 => 7, //  16 384 Hz : tous les 256
            _ => unreachable!(),
        };
        self.tac & TAC_ENABLE != 0 && (self.counter >> bit) & 1 == 1
    }

    /// Fait avancer TIMA si le signal vient de passer de 1 à 0.
    fn detect_fall(&mut self, before: bool) {
        if before && !self.signal() {
            self.increment_tima();
        }
    }

    fn increment_tima(&mut self) {
        let (value, overflow) = self.tima.overflowing_add(1);
        if overflow {
            self.tima = self.tma;
            self.interrupt = true;
        } else {
            self.tima = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un timer en marche à la vitesse `speed` (0 à 3).
    fn timer_at(speed: u8) -> Timer {
        let mut timer = Timer::new();
        timer.write(0xFF07, TAC_ENABLE | speed);
        timer
    }

    #[test]
    fn div_augmente_tous_les_256_cycles() {
        let mut timer = Timer::new();
        timer.tick(252);
        assert_eq!(timer.read(0xFF04), 0);
        timer.tick(4);
        assert_eq!(timer.read(0xFF04), 1);
        timer.tick(256 * 3);
        assert_eq!(timer.read(0xFF04), 4);
    }

    #[test]
    fn ecrire_dans_div_le_remet_a_zero() {
        let mut timer = Timer::new();
        timer.tick(1000);
        timer.write(0xFF04, 0x42);
        assert_eq!(timer.read(0xFF04), 0);
    }

    #[test]
    fn les_quatre_vitesses() {
        for (speed, period) in [(0, 1024), (1, 16), (2, 64), (3, 256)] {
            let mut timer = timer_at(speed);
            timer.tick(period - 4);
            assert_eq!(timer.read(0xFF05), 0, "vitesse {speed} : trop tôt");
            timer.tick(4);
            assert_eq!(timer.read(0xFF05), 1, "vitesse {speed} : pile à l'heure");
            timer.tick(period * 9);
            assert_eq!(timer.read(0xFF05), 10, "vitesse {speed} : 10 pas");
        }
    }

    #[test]
    fn eteint_il_ne_bouge_pas() {
        let mut timer = Timer::new();
        timer.write(0xFF07, 0x01); // vitesse rapide, mais interrupteur à 0
        timer.tick(1000);
        assert_eq!(timer.read(0xFF05), 0);
    }

    #[test]
    fn le_debordement_recharge_tma_et_sonne() {
        let mut timer = timer_at(1);
        timer.write(0xFF06, 0xC0); // TMA
        timer.write(0xFF05, 0xFF); // TIMA, à un pas du débordement
        assert!(!timer.tick(12));
        assert!(timer.tick(4));
        assert_eq!(timer.read(0xFF05), 0xC0);
        assert!(!timer.tick(4)); // la sonnerie n'est transmise qu'une fois
    }

    #[test]
    fn remettre_div_a_zero_peut_faire_avancer_tima() {
        let mut timer = timer_at(1); // surveille le bit 3
        timer.tick(8); // compteur = 8 : bit 3 = 1
        timer.write(0xFF04, 0); // compteur = 0 : bit 3 tombe → front descendant
        assert_eq!(timer.read(0xFF05), 1);
    }

    #[test]
    fn eteindre_le_timer_peut_faire_avancer_tima() {
        let mut timer = timer_at(1);
        timer.tick(8); // bit 3 = 1
        timer.write(0xFF07, 0x01); // interrupteur à 0 : le signal ET tombe
        assert_eq!(timer.read(0xFF05), 1);
    }
}
