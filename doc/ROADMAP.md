# RetroEmu — Feuille de route

Plan en 11 étapes pour réaliser l'émulateur Game Boy (DMG + CGB) demandé par le sujet (v9.1), en Rust avec le crate `sdl2`.

---

## 1. Comprendre l'architecture de la Game Boy
- **À apprendre :** comment les composants communiquent. Le CPU (Sharp SM83, proche du Z80) lit et écrit tout via un bus mémoire. La PPU, les timers, les entrées et la cartouche sont tous *memory-mapped*, c'est-à-dire accessibles à des adresses précises.
- **À lire :** [Pan Docs](https://gbdev.io/pandocs/) en entier, une première fois en survol : *Memory Map*, *I/O Registers*, *Interrupts*. C'est la bible du projet.
- **À faire :** dessiner la memory map de `0x0000` à `0xFFFF` (ROM, VRAM, WRAM, OAM, I/O, HRAM, IE) et la garder sous la main.

## 2. Concevoir l'architecture du code
- **À apprendre :** l'ownership en Rust, qui pose problème quand plusieurs composants doivent accéder au même bus. Le schéma classique : `Cpu` possède un `Bus`, et le `Bus` possède `Ppu`, `Timer`, `Joypad` et `Cartridge`.
- **À faire :** créer les modules `bus.rs`, `cartridge.rs`, `ppu.rs`, `timer.rs`, `joypad.rs` et un trait `Mbc`. Prévoir dès maintenant une boucle `step()` qui renvoie le nombre de cycles consommés. Tout le timing en dépend.

## 3. Charger une ROM et lire le header
- **À rechercher :** le *Cartridge Header* (`0x0100` à `0x014F`) : titre, type de cartouche (`0x0147`), taille ROM/RAM, flag CGB (`0x0143`).
- **À faire :** charger un `.gb`, afficher son header, et choisir l'implémentation de MBC selon le type. Commencer par *ROM only*.

## 4. Implémenter le CPU (la plus grosse partie)
- **À apprendre :** les registres A, F, B, C, D, E, H, L, SP et PC, les paires (AF, BC, DE, HL) et les flags Z, N, H, C. Le flag H (*half-carry*) est le piège classique.
- **Références :** la [table des opcodes](https://gbdev.io/gb-opcodes/optables/) et la doc des 256 opcodes plus les 256 préfixés `CB`. Regarder comment on décode par motifs de bits : ça évite d'écrire 512 `match` à la main.
- **À faire :** implémenter toutes les instructions avec leur nombre exact de cycles, plus `HALT`, `STOP`, `EI`/`DI` (avec son délai d'une instruction) et les interruptions (IE/IF, vecteurs `0x40` à `0x60`).
- **Comment valider :** les ROMs de test **Blargg `cpu_instrs`**, qui affichent leurs résultats sur le port série (`0xFF01`/`0xFF02`). On peut donc les valider dans le terminal avant même d'avoir un écran. Pour comparer instruction par instruction, utiliser les logs de [gameboy-doctor](https://github.com/robert/gameboy-doctor).

## 5. Construire le debugger (obligatoire)
- **Ce que demande le sujet :** afficher les registres, afficher la prochaine instruction, exécuter une instruction (*step*), exécuter une frame et une seconde d'émulation.
- **À faire :** écrire un petit désassembleur, qui réutilise les tables de l'étape 4, puis un mode pas-à-pas. Le faire **tôt** : il servira à déboguer tout le reste. Il peut être dans le terminal ou dans une fenêtre SDL.

## 6. Timers et interruptions
- **À rechercher :** DIV, TIMA, TMA, TAC (*Pan Docs → Timer*), et la façon dont DIV est un compteur interne de 16 bits dont TIMA dépend.
- **À faire :** faire avancer les timers à chaque `step` avec les cycles du CPU, et déclencher l'interruption Timer.
- **Comment valider :** Blargg `instr_timing`, puis les tests Mooneye `timer/`.

## 7. La PPU (l'affichage)
- **À apprendre :** les modes PPU (OAM scan, drawing, HBlank, VBlank : 456 cycles par ligne, 154 lignes), LY/LYC, STAT, LCDC, le format des tiles (2 bits par pixel, 16 octets par tile), les tile maps, SCX/SCY, la window (WX/WY), les sprites (OAM, 10 par ligne, priorités, flips), les palettes BGP/OBP0/OBP1 et l'OAM DMA (`0xFF46`).
- **À faire :** un rendu ligne par ligne (*scanline renderer*) dans un framebuffer 160×144, affiché ensuite dans une texture SDL2. Pas besoin d'émuler le FIFO pixel par pixel pour la partie obligatoire.
- **Comment valider :** **`acid2/dmg-acid2.gb`** dans le bundle fourni. C'est le test de référence pour background, window, sprites et palettes.

## 8. Boucle principale, timing et GUI
- **À apprendre :** une frame dure 70 224 cycles, soit environ 59,73 Hz. Il faut synchroniser sur le temps réel (sleep, ou vsync SDL) sans figer l'interface. Le sujet impose aussi une GUI avec au minimum *load / play / pause*.
- **À rechercher :** soit faire tourner l'émulation dans un thread séparé (communication par channels `std::sync::mpsc`), soit une boucle unique qui exécute une frame puis traite les événements SDL. L'exigence « sans affecter la GUI » doit être défendable en évaluation.
- **À faire :** les boutons load/play/pause. Comme SDL2 n'a pas de widgets, il faut les dessiner soi-même : texte via `sdl2::ttf`, ou un menu minimal. Prévoir aussi un sélecteur de fichier, même basique.

## 9. Les entrées (joypad)
- **À rechercher :** le registre `0xFF00` (P1/JOYP), avec la sélection directions ou boutons par les bits 4-5, en logique inversée (0 = appuyé), et l'interruption Joypad.
- **À faire :** associer les touches clavier SDL aux 8 boutons.

## 10. Les MBC et les sauvegardes
- **À apprendre :** le *bank switching*. Lire *Pan Docs → MBCs* :
  - **MBC1** : mode banking 0/1, banques ROM et RAM, et le cas du « bank 0 → 1 » ;
  - **MBC2** : 512×4 bits de RAM intégrée, sélection selon le bit 8 de l'adresse ;
  - **MBC5** : banque ROM sur 9 bits, jusqu'à 16 banques RAM.
- **À faire :** quand la cartouche a une batterie, écrire la RAM dans un fichier `.sav` à la fermeture (et éventuellement à intervalles réguliers), puis la recharger au lancement.
- **Comment valider :** les tests **Mooneye `mbc1/`, `mbc2/`, `mbc5/`** du bundle (par exemple `mooneye/mbc5/rom_2Mb.gb`).

## 11. Le mode couleur (CGB)
- **À apprendre :** les différences avec la DMG (*Pan Docs → CGB Registers*) :
  - palettes couleur BCPS/BCPD/OCPS/OCPD (15 bits RGB) ;
  - 2 banques de VRAM (VBK) et attributs de tiles, stockés en banque 1 ;
  - 8 banques de WRAM (SVBK) ;
  - double-speed (KEY1 + `STOP`) ;
  - HDMA/GDMA (`0xFF51` à `0xFF55`) ;
  - priorités BG/OAM propres au CGB.
- **À faire :** détecter le mode via le header et initialiser les registres post-boot correspondants.
- **Comment valider :** **`cgb-acid2.gbc`**.

---

## En parallèle, tout au long du projet
- **Build reproductible sous Linux :** documenter `cargo build --release`, la dépendance `libsdl2-dev`, ou utiliser la feature `bundled` du crate `sdl2`. Tester dans une VM ou un Docker Linux (développement sur macOS).
- **Aucune ROM commerciale dans le dépôt.** Ajouter aussi `target/` au `.gitignore`.
- **Bonus :** ils ne comptent que si la partie obligatoire est *parfaite*. La terminer d'abord. Les bonus les plus rentables ensuite : save states (facile si l'état est sérialisable, par exemple avec `serde`), forcer DMG/CGB, et l'UX (reset, vitesse, drag & drop).

## Ressources clés
- [Pan Docs](https://gbdev.io/pandocs/)
- [gbdev.io](https://gbdev.io/)
- [Awesome GB dev](https://github.com/gbdev/awesome-gbdev)
- *The Ultimate Game Boy Talk* (conférence vidéo de Michael Steil), idéale pour démarrer
- Le code source de **SameBoy** pour les cas tordus
