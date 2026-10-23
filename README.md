# desmack

A command-line tool that extracts the assets of a Super Mario Kart SNES ROM: driver sprites,
items, hazards, HUD, font, screens, tilesets, skies and course maps.

desmack ships no game data. Bring your own ROM (Japan, USA or Europe).

## Usage

```sh
desmack smk.sfc              # pick categories in a dialog
desmack --all smk.sfc        # everything, no dialog
desmack --maps --tilesets *.sfc
```

Each ROM's assets go to a folder next to it, named after the ROM. `desmack --help` lists every
flag.

Shell completions:

```sh
desmack --completions fish > ~/.config/fish/completions/desmack.fish
```

## Credits

desmack is written from documented facts about the ROM. Its research sources are
Epic Edit, the SMK track editor, and Stifu's Super Mario
Kart ROM documentation. No Epic Edit code is used.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.
