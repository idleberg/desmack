# desmack

> A command-line tool that extracts the assets of a SMK ROM (SNES)

![Crates.io License](https://img.shields.io/crates/l/desmack?style=for-the-badge)
[![Crates.io Version](https://img.shields.io/crates/v/desmack?style=for-the-badge)](https://crates.io/crates/desmack)
[![CI](https://img.shields.io/github/actions/workflow/status/idleberg/desmack/ci.yml?style=for-the-badge)](https://github.com/idleberg/desmack/actions)

The CLI extracts any of the following:

- driver sprites
- items
- hazards
- HUD
- font
- screens
- tilesets
- skies
- course maps
- audio samples
- sound effects, as `.spc` files

> [!NOTE]
> `desmack` ships no game data. Bring your own ROM (Japan, USA or Europe).

## Installation

### Cargo

```
cargo install desmack
```

### Homebrew

```
brew install idleberg/asahi/desmack
```

## Usage

```sh
desmack smk.sfc              # pick categories in a dialog
desmack --all smk.sfc        # everything, no dialog
desmack --maps --tilesets smk.sfc
```

The assets go to a folder next to the ROM, named after it. `desmack --help` lists every
flag.

Shell completions:

```sh
desmack --completions fish > ~/.config/fish/completions/desmack.fish
```

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.
