//! Outils partagés par les tests du CPU.

use super::Cpu;
use crate::bus::Bus;
use crate::cartridge::Cartridge;

/// Bus avec une ROM vide, et `program` placé à l'entry point 0x0100.
pub(super) fn bus_with(program: &[u8]) -> Bus {
    let mut rom = vec![0; 0x8000];
    rom[0x0100..0x0100 + program.len()].copy_from_slice(program);
    Bus::new(Cartridge::from_bytes(rom).unwrap())
}

/// Exécute `count` instructions et renvoie le total de T-cycles.
pub(super) fn run(cpu: &mut Cpu, bus: &mut Bus, count: usize) -> u32 {
    (0..count).map(|_| cpu.step(bus).unwrap()).sum()
}
