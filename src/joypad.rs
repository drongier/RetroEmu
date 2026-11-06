//! Joypad : les 8 boutons, lus par le jeu via le registre P1/JOYP (0xFF00).
//!
//! Les boutons sont rangés en 2 rangées de 4. Le jeu choisit une rangée (bits 4-5),
//! puis lit l'état de ses 4 boutons (bits 0-3). Partout, 0 veut dire « choisi » ou « enfoncé ».

#[derive(Debug, Clone, Copy)]
pub enum Button {
    Right,
    Left,
    Up,
    Down,
    A,
    B,
    Select,
    Start,
}

pub struct Joypad {
    /// Bits 4-5 de P1, écrits par le jeu pour choisir la rangée (0 = choisie).
    select: u8,
    /// Un bit par bouton, 1 = enfoncé. Bits 0-3 : directions, bits 4-7 : actions.
    pressed: u8,
}

impl Joypad {
    pub fn new() -> Joypad {
        Joypad {
            select: 0x30, // aucune rangée choisie
            pressed: 0,
        }
    }

    pub fn read(&self) -> u8 {
        let mut buttons = 0x0F; // 1 = relâché
        if self.select & 0x10 == 0 {
            buttons &= !(self.pressed & 0x0F); // rangée directions
        }
        if self.select & 0x20 == 0 {
            buttons &= !(self.pressed >> 4); // rangée actions
        }
        0xC0 | self.select | buttons // les bits 6-7 n'existent pas et se lisent à 1
    }

    pub fn write(&mut self, value: u8) {
        self.select = value & 0x30; // seuls les bits 4-5 sont écrits par le jeu
    }

    /// Renvoie `true` si le bouton vient d'être enfoncé (le Bus lèvera l'interruption Joypad).
    pub fn press(&mut self, button: Button) -> bool {
        let bit = Joypad::bit(button);
        let newly_pressed = self.pressed & bit == 0;
        self.pressed |= bit;
        newly_pressed
    }

    pub fn release(&mut self, button: Button) {
        self.pressed &= !Joypad::bit(button);
    }

    fn bit(button: Button) -> u8 {
        match button {
            Button::Right => 0x01,
            Button::Left => 0x02,
            Button::Up => 0x04,
            Button::Down => 0x08,
            Button::A => 0x10,
            Button::B => 0x20,
            Button::Select => 0x40,
            Button::Start => 0x80,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_rangee_choisie_tout_est_relache() {
        let mut joypad = Joypad::new();
        joypad.press(Button::A);
        assert_eq!(joypad.read(), 0xFF);
    }

    #[test]
    fn lit_la_rangee_choisie() {
        let mut joypad = Joypad::new();
        joypad.press(Button::A);
        joypad.press(Button::Down);

        joypad.write(0x10); // bit 5 = 0 : rangée actions
        assert_eq!(joypad.read(), 0xDE); // 1101 1110 : A enfoncé

        joypad.write(0x20); // bit 4 = 0 : rangée directions
        assert_eq!(joypad.read(), 0xE7); // 1110 0111 : Bas enfoncé
    }

    #[test]
    fn press_signale_seulement_un_nouvel_appui() {
        let mut joypad = Joypad::new();
        assert!(joypad.press(Button::Start));
        assert!(!joypad.press(Button::Start)); // déjà enfoncé
        joypad.release(Button::Start);
        assert!(joypad.press(Button::Start));
    }
}
