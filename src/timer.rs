//! Timer : registres DIV, TIMA, TMA et TAC (0xFF04–0xFF07).
//!
//! Pour l'instant seul DIV avance. TIMA et l'interruption Timer arrivent à l'étape 6.

#[derive(Default)]
pub struct Timer {
    /// Compteur interne de 16 bits, +1 à chaque T-cycle. DIV en est l'octet haut.
    counter: u16,
    tima: u8,
    tma: u8,
    tac: u8,
}

impl Timer {
    pub fn new() -> Timer {
        Timer::default()
    }

    /// Avance de `cycles` T-cycles. Renvoie `true` si le timer demande une interruption.
    pub fn tick(&mut self, cycles: u32) -> bool {
        self.counter = self.counter.wrapping_add(cycles as u16);
        false
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
            0xFF04 => self.counter = 0, // écrire n'importe quoi dans DIV le remet à zéro
            0xFF05 => self.tima = value,
            0xFF06 => self.tma = value,
            0xFF07 => self.tac = value & 0x07,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn div_augmente_tous_les_256_cycles() {
        let mut timer = Timer::new();
        timer.tick(255);
        assert_eq!(timer.read(0xFF04), 0);
        timer.tick(1);
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
}
