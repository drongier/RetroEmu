//! Désassembleur : le chemin inverse du CPU. Il transforme les octets d'une instruction
//! en texte lisible (« LD A, B ») et donne sa longueur.
//!
//! Il réutilise le décodage par bits de `step` et ne fait que lire la mémoire :
//! regarder une instruction ne doit jamais changer l'état de la console.

use crate::bus::Bus;

const R8: [&str; 8] = ["B", "C", "D", "E", "H", "L", "(HL)", "A"];
const R16: [&str; 4] = ["BC", "DE", "HL", "SP"];
/// Pour PUSH et POP, le code 3 désigne AF.
const R16_STACK: [&str; 4] = ["BC", "DE", "HL", "AF"];
const R16_INDIRECT: [&str; 4] = ["(BC)", "(DE)", "(HL+)", "(HL-)"];
const ALU: [&str; 8] = ["ADD", "ADC", "SUB", "SBC", "AND", "XOR", "OR", "CP"];
const CC: [&str; 4] = ["NZ", "Z", "NC", "C"];
const ROT: [&str; 8] = ["RLC", "RRC", "RL", "RR", "SLA", "SRA", "SWAP", "SRL"];

/// Désassemble l'instruction à `addr`. Renvoie son texte et sa longueur en octets.
pub fn disassemble(bus: &Bus, addr: u16) -> (String, u16) {
    let op = bus.read8(addr);
    let n8 = bus.read8(addr.wrapping_add(1));
    let n16 = bus.read16(addr.wrapping_add(1));
    let e8 = n8 as i8;
    // Destination d'un JR : comptée depuis la fin de l'instruction (2 octets).
    let jr_target = addr.wrapping_add(2).wrapping_add(e8 as i16 as u16);

    // Les bouts de l'opcode, découpés comme dans le CPU.
    let y = ((op >> 3) & 0x07) as usize; // bits 5-3
    let z = (op & 0x07) as usize; //        bits 2-0
    let p = ((op >> 4) & 0x03) as usize; // bits 5-4 : la paire de registres
    let cc = CC[((op >> 3) & 0x03) as usize];

    let (text, len) = match op {
        0x00 => ("NOP".to_string(), 1),

        // --- LD ---
        0x76 => ("HALT".to_string(), 1),
        0x40..=0x7F => (format!("LD {}, {}", R8[y], R8[z]), 1),
        0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => {
            (format!("LD {}, {n8:#04X}", R8[y]), 2)
        }
        0x01 | 0x11 | 0x21 | 0x31 => (format!("LD {}, {n16:#06X}", R16[p]), 3),
        0x02 | 0x12 | 0x22 | 0x32 => (format!("LD {}, A", R16_INDIRECT[p]), 1),
        0x0A | 0x1A | 0x2A | 0x3A => (format!("LD A, {}", R16_INDIRECT[p]), 1),
        0xE0 => (format!("LDH ({:#06X}), A", 0xFF00 | n8 as u16), 2),
        0xF0 => (format!("LDH A, ({:#06X})", 0xFF00 | n8 as u16), 2),
        0xE2 => ("LD (0xFF00+C), A".to_string(), 1),
        0xF2 => ("LD A, (0xFF00+C)".to_string(), 1),
        0xEA => (format!("LD ({n16:#06X}), A"), 3),
        0xFA => (format!("LD A, ({n16:#06X})"), 3),
        0x08 => (format!("LD ({n16:#06X}), SP"), 3),
        0xF9 => ("LD SP, HL".to_string(), 1),

        // --- ALU 8 bits ---
        0x80..=0xBF => (format!("{} A, {}", ALU[y], R8[z]), 1),
        0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
            (format!("{} A, {n8:#04X}", ALU[y]), 2)
        }
        0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C => (format!("INC {}", R8[y]), 1),
        0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D => (format!("DEC {}", R8[y]), 1),

        // --- Calculs 16 bits ---
        0x03 | 0x13 | 0x23 | 0x33 => (format!("INC {}", R16[p]), 1),
        0x0B | 0x1B | 0x2B | 0x3B => (format!("DEC {}", R16[p]), 1),
        0x09 | 0x19 | 0x29 | 0x39 => (format!("ADD HL, {}", R16[p]), 1),
        0xE8 => (format!("ADD SP, {e8}"), 2),
        0xF8 => (format!("LD HL, SP{e8:+}"), 2),

        // --- Sauts ---
        0xC3 => (format!("JP {n16:#06X}"), 3),
        0xC2 | 0xCA | 0xD2 | 0xDA => (format!("JP {cc}, {n16:#06X}"), 3),
        0xE9 => ("JP HL".to_string(), 1),
        0x18 => (format!("JR {jr_target:#06X}"), 2),
        0x20 | 0x28 | 0x30 | 0x38 => (format!("JR {cc}, {jr_target:#06X}"), 2),
        0xCD => (format!("CALL {n16:#06X}"), 3),
        0xC4 | 0xCC | 0xD4 | 0xDC => (format!("CALL {cc}, {n16:#06X}"), 3),
        0xC9 => ("RET".to_string(), 1),
        0xC0 | 0xC8 | 0xD0 | 0xD8 => (format!("RET {cc}"), 1),
        0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
            (format!("RST {:#04X}", op & 0x38), 1)
        }

        // --- Pile ---
        0xC5 | 0xD5 | 0xE5 | 0xF5 => (format!("PUSH {}", R16_STACK[p]), 1),
        0xC1 | 0xD1 | 0xE1 | 0xF1 => (format!("POP {}", R16_STACK[p]), 1),

        // --- Préfixe CB ---
        0xCB => (disassemble_cb(n8), 2),

        // --- Interruptions et veille ---
        0xF3 => ("DI".to_string(), 1),
        0xFB => ("EI".to_string(), 1),
        0xD9 => ("RETI".to_string(), 1),
        0x10 => ("STOP".to_string(), 2),

        // --- Divers ---
        0x2F => ("CPL".to_string(), 1),
        0x37 => ("SCF".to_string(), 1),
        0x3F => ("CCF".to_string(), 1),
        0x27 => ("DAA".to_string(), 1),
        0x07 | 0x0F | 0x17 | 0x1F => (["RLCA", "RRCA", "RLA", "RRA"][y].to_string(), 1),

        // --- Les 11 cases vides ---
        0xD3 | 0xDB | 0xDD | 0xE3 | 0xE4 | 0xEB | 0xEC | 0xED | 0xF4 | 0xFC | 0xFD => {
            (format!("??? {op:#04X} (opcode illégal)"), 1)
        }
    };
    (text, len)
}

/// Le second octet d'une instruction CB : xx yyy zzz.
fn disassemble_cb(op: u8) -> String {
    let y = ((op >> 3) & 0x07) as usize;
    let r = R8[(op & 0x07) as usize];
    match op >> 6 {
        0 => format!("{} {r}", ROT[y]),
        1 => format!("BIT {y}, {r}"),
        2 => format!("RES {y}, {r}"),
        3 => format!("SET {y}, {r}"),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::bus_with;
    use super::*;

    /// Désassemble la première instruction de `bytes` (placée en 0x0100).
    fn dis(bytes: &[u8]) -> (String, u16) {
        disassemble(&bus_with(bytes), 0x0100)
    }

    #[test]
    fn instructions_d_un_octet() {
        assert_eq!(dis(&[0x00]), ("NOP".to_string(), 1));
        assert_eq!(dis(&[0x78]), ("LD A, B".to_string(), 1));
        assert_eq!(dis(&[0x76]), ("HALT".to_string(), 1)); // pas « LD (HL), (HL) »
        assert_eq!(dis(&[0xAF]), ("XOR A, A".to_string(), 1));
        assert_eq!(dis(&[0x2A]), ("LD A, (HL+)".to_string(), 1));
        assert_eq!(dis(&[0xF5]), ("PUSH AF".to_string(), 1));
        assert_eq!(dis(&[0xEF]), ("RST 0x28".to_string(), 1));
    }

    #[test]
    fn instructions_avec_operande() {
        assert_eq!(dis(&[0x06, 0x42]), ("LD B, 0x42".to_string(), 2));
        assert_eq!(dis(&[0x21, 0x00, 0xC0]), ("LD HL, 0xC000".to_string(), 3));
        assert_eq!(dis(&[0xF0, 0x44]), ("LDH A, (0xFF44)".to_string(), 2));
        assert_eq!(dis(&[0xC2, 0x50, 0x01]), ("JP NZ, 0x0150".to_string(), 3));
        assert_eq!(dis(&[0xE8, 0xFE]), ("ADD SP, -2".to_string(), 2));
        assert_eq!(dis(&[0xF8, 0x05]), ("LD HL, SP+5".to_string(), 2));
    }

    #[test]
    fn jr_affiche_la_destination() {
        // JR NZ, -6 en 0x0100 : 0x0102 − 6 = 0x00FC
        assert_eq!(dis(&[0x20, 0xFA]), ("JR NZ, 0x00FC".to_string(), 2));
    }

    #[test]
    fn instructions_cb() {
        assert_eq!(dis(&[0xCB, 0x7C]), ("BIT 7, H".to_string(), 2));
        assert_eq!(dis(&[0xCB, 0x37]), ("SWAP A".to_string(), 2));
        assert_eq!(dis(&[0xCB, 0xFE]), ("SET 7, (HL)".to_string(), 2));
    }

    #[test]
    fn opcode_illegal() {
        assert_eq!(dis(&[0xD3]), ("??? 0xD3 (opcode illégal)".to_string(), 1));
    }
}
