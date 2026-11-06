//! Script de build : indique au linker où se trouve la bibliothèque SDL2.

use std::process::Command;

fn main() {
    // sdl2-config est installé avec SDL2 (Homebrew, apt...) et connaît son préfixe d'installation.
    if let Ok(output) = Command::new("sdl2-config").arg("--prefix").output() {
        if output.status.success() {
            let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("cargo:rustc-link-search=native={prefix}/lib");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
}
