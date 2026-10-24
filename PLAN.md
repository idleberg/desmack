# desmack v0.1 plan

desmack is a Rust CLI that extracts the assets of a Super Mario Kart SNES ROM. Terms follow
[GLOSSARY.md](GLOSSARY.md).

## Decisions

### ROMs

- desmack supports the Japan, USA and Europe Regions. All three known dumps are Revision 1.0.
- A file is accepted if its header says it is SMK. A copier header is removed before reading.
- desmack compares the CRC32 of the ROM, without the copier header, to the known dumps:

  | Region | CRC32 |
  |---|---|
  | Japan | `C8002453` |
  | USA | `CD80DB86` |
  | Europe | `56410E5E` |

- **Modified ROMs**: desmack reads the Region from `$FFD9` and uses that Region's offsets. A
  category it can't decode is reported as failed, and the other categories are still extracted.
- **Report**: each ROM gets one line with its Region, Revision, whether it is known or modified,
  and the categories extracted or failed. There is no `--info` flag.

### CLI

```
desmack [FLAGS] <ROM>...
  --drivers --items --hazards --cast --effects
  --hud --font --screens --tilesets --skies --maps
  --all          every category, no dialog
  --force        overwrite without asking
  --completions <SHELL>   print a completion script to stdout and exit (as in ardent)
```

- **No category flag, stdin is a TTY**: one clark `group_multiselect` dialog for all ROMs. Its
  groups are sprites, graphics, textures and maps, and every category starts selected.
- **No category flag, no TTY**: everything is extracted.
- **Output**: `<rom dir>/<rom stem>/`, next to each ROM.
- **Same file passed twice**: desmack resolves the real path and uses the file once, without saying so.
- **Two ROMs, one output folder** (e.g. `smk.sfc` and `smk.smc`): both are reported and skipped.
  An output folder desmack can't write to is reported and skipped the same way. The other ROMs
  continue either way.
- **Output folder already exists**:
  - with a TTY, one clark confirm per folder;
  - with `--force`, it is overwritten;
  - with no TTY and no `--force`, the ROM is reported and skipped.
- **Exit code**: non-zero if any ROM failed or was skipped.

### Output layout

```
<rom stem>/
  sprites/drivers/<driver>/000.png …
  sprites/items/ sprites/hazards/ sprites/cast/ sprites/effects/
  graphics/hud/ graphics/font/ graphics/screens/
  textures/tilesets/<theme>.png
  textures/skies/<theme>-near.png  <theme>-far.png
  maps/<course>/gp.map tt.map hazards.txt ai.txt start.txt
  maps/genres/<theme>.txt
```

- **Names**: courses, themes and drivers are lowercase and hyphenated, e.g. `mario-circuit-1`,
  `dk-jr`.
- **PNGs**: indexed, with colour 0 transparent.
- **Sprites**: one PNG per frame, numbered in ROM order. Frames get real names (e.g. `angle-00`)
  once the Mesen research identifies them.
- **Tilesets**: one 128×128 PNG per Theme, 16×16 tiles. Tile index = row × 16 + column, and
  Ground is cell 0. The palette has 256 entries, one block of 16 for each of the 16 palettes, so
  every tile is drawn with its own palette.
- **Skies**: each layer is drawn as it appears in game, not dumped as raw tiles: near is
  1024×32, far 512×32. Colour 0 stays transparent; the game shows
  CGRAM colour 0 behind both.
- **`gp.map`, `tt.map`**: 128 lines, each holding 128 hex tile indices separated by spaces. The GP
  map includes the Overlay and the TT map doesn't.
- **`hazards.txt`**: one `x y` per line, in tiles, with `match-race` appended for Match Race
  entries. The hazard type follows from the Theme. Battle courses have none.
- **`ai.txt`**: one line per AI zone: its shape and size in tiles, followed by the zone's AI
  target `x y speed`, with `intersection` appended when the flag is set.
- **`start.txt`**: one `x y` per Start position, in pixels: 8 on GP courses, 2 on battle courses.
- **`genres/<theme>.txt`**: 256 lines, `<hex index> <genre name>`, with Epic Edit's genre names.
  An index the ROM doesn't define is `undefined`.

### Project

- **Licence**: `MIT OR Apache-2.0`, with `LICENSE-MIT` and `LICENSE-APACHE`. The code is written
  from documented facts, and Epic Edit and Stifu's ROM doc are credited in the README as research
  sources. No Epic Edit (GPL-3.0) code is translated.
- **Crates**:
  - `clap` 4: derive, `default-features = false`, features as in installua
  - `clap_complete` 4
  - `clark` 0.2
  - `png`
  - `crc32fast`
  - `anyhow`

  TTY detection uses `std::io::IsTerminal`.
- **Cargo**: edition 2024, binary only, `[profile.release] lto = true, strip = true`.
- **Tooling**, copied from ardent (`~/Repositories/_nsis/ardent`):
  - mise tasks `build`, `format`, `format:check`, `lint`, `test`
  - `hk.pkl` pre-commit hook
  - `.github/workflows/ci.yml` and `release.yml`
- **Tests**:
  - synthetic unit tests for header detection, the decompressor and the 4bpp/BGR555 decoders;
  - integration tests that run only when `SMK_ROM` is set.

  No ROM data goes into the repo, not even as test fixtures.
- **Out of scope**: switching kart-prototype to use desmack's output, including its missing Mario
  Circuit pipes, comes after v0.1.

## Milestones

1. **Scaffold**: Cargo project, tooling, CI, licences, README.
   - CLI parsing, `--completions`, ROM loading and the folder and overwrite rules, all with no
     categories yet.
2. **ROM detection**: copier header, header check, Region, Revision, known or modified, and the
   report line.
3. **Decompressor**: the command format below, tested with synthetic streams and against
   track 0 (3372 → 5871 → 16384 bytes).
4. **Maps**: tile maps, Overlay, genres, hazard positions, AI zones and targets, Start positions,
   for all 24 courses (20 GP and 4 battle).
5. **Tilesets and skies**.
6. **Items and hazards**.
7. **Mesen research**, then extraction, for drivers, cast, effects, HUD, font and screens.
8. **Release v0.1**.

## Known ROM facts

All offsets are file offsets in the USA ROM unless noted, with any copier header removed. The ROM
is HiROM: file offset = `(bank & 0x3F) << 16 | addr`.

### Header

- Title `SUPER MARIO KART` at `$FFC0`.
- `$FFD5 = 0x31`, `$FFD6 = 0x05`, `$FFD8 = 0x01`.
- Region at `$FFD9`: 0 = Japan, 1 = USA, 2 = Europe. Version at `$FFDB`.
- Copier header present when `size % 0x40000 == 512`.

### Compression

Read a byte `v`. `0xFF` ends the stream. Otherwise the command is `v >> 5` and the count is
`(v & 0x1F) + 1`. Command 7 means a long header: the command is `(v >> 2) & 7` and the count is
`((v & 3) << 8 | next) + 1`.

| Command | Action |
|---|---|
| 0 | copy n literal bytes |
| 1 | repeat one byte |
| 2 | alternate two bytes |
| 3 | write an incrementing byte |
| 4 | copy from absolute output offset (u16 LE) |
| 5 | as 4, each byte XOR `0xFF` |
| 6 | copy from `out_pos - next byte` |

A copy from outside the output reads stale RAM in the game; desmack writes 0. Only Bowser
Castle's road tiles do this, for 2 bytes in a tile no course uses.

Track maps and the podium are compressed twice. Driver sprites are not compressed. Pointers are
3 bytes LE: `(b2 & 0x0F) << 16 | b1 << 8 | b0`.

### Courses

The course byte gives this order:

| Index | Course | Index | Course | Index | Course |
|---|---|---|---|---|---|
| 0 | MC3 | 7 | MC1 | 14 | MC4 |
| 1 | GV2 | 8 | GV3 | 15 | MC2 |
| 2 | DP2 | 9 | BC3 | 16 | GV1 |
| 3 | BC2 | 10 | CI2 | 17 | BC1 |
| 4 | VL2 | 11 | DP3 | 18 | CI1 |
| 5 | RR | 12 | VL1 | 19 | DP1 |
| 6 | KB2 | 13 | KB1 | | |

20–23 are Battle Courses 3, 4, 1 and 2.

**Track list**:
- The pointer is at `0x1E749`; Europe `0x1E738`, Japan `0x1E74D`.
- It leads to the pointer tables for the 24 maps, then 8 entries each for road graphics, palettes,
  object graphics, background graphics and background layouts.
- Then come the GP order and 24 theme bytes (theme = byte >> 1).
- Theme numbers: 0 GV, 1 MC, 2 DP, 3 CI, 4 VL, 5 KB, 6 BC, 7 RR.

**Course data**:

| Data | Offset |
|---|---|
| Genres | `0x7FDBA` |
| Hazard positions | `0x5C800` + course × 64 |
| Object zones | `0x4DB93` |
| GV pillar pointers | `0x4DD15` |
| AI bank byte | `0x1FBD3`; Europe `0x1FB9D`, Japan `0x1FBC4` |
| AI zone pointers | `0x1FF9B`; Europe `0x1FF6D`, Japan `0x1FF8C` |
| Start pointers | `0x18A79`; Europe `0x18A92`, Japan `0x18A8D` |
| Lap line | `0x180D4` + course × 6 |
| Overlay | `0x5D000` + course × 128 |
| Overlay pattern pointers | `0x4F23D`; Europe `0x4F159`, Japan `0x4F0B5` |
| Overlay pattern sizes | `0x4F384`; Europe `0x4F2A0`, Japan `0x4F1FC` |

- **Genres**: one compressed stream. A Theme's genres are 192 bytes from its offset, a u16 in a
  table 16 bytes before the track list, then the stream's first 64 bytes. Koopa Beach and
  Rainbow Road run past the stream's end; their courses don't use those tiles.
- **Overlay**: 3-byte entries, `FF FF FF` ending the list. Pattern = `b0 & 0x3F`, a u16 pointer
  into bank 4; size = `b0 >> 6`, as (w, h): 2×2, 3×1, 1×3, 5×5. A pattern byte `0xFF` is
  transparent. The tile offset is `(b1 | b2 << 8) & 0x3FFF`.
- **GV pillars**: 3 u16 pointers into bank 4, for GV1, GV2 and GV3. Each list is a 2-byte header,
  then `x y` bytes in tiles, ending with `00 00`.

- **Hazard positions**: 16 regular entries plus 6 Match Race entries, 2 bytes each, in tiles:
  `x = b0 & 0x7F`, `y = (b1 & 0x3F) << 1 | b0 >> 7`.
- **Hazard type per Theme**:
  - GV: pillar
  - MC and VL: pipe; DP1's pipe is orange (palette 5)
  - DP: Monty Mole
  - CI: Piranha Plant
  - KB: Cheep Cheep
  - BC: Thwomp
  - RR: moving Thwomp
- **AI**: the target pointers start 0x30 bytes after the zone pointers. A zone is a shape byte,
  then values in 2-tile units, with `0xFF` ending the list: shape 0 is a rectangle `x y w h`;
  2, 4, 6 and 8 are triangles `x y leg`, whose right-angle corner (x, y) is at the top left,
  top right, bottom right and bottom left. A target is 3 bytes: x and y in tiles, then the speed
  in bits 0–1 and the intersection flag in bit 7.
- **Start positions**: a u16 per course in bank 1, to a struct whose first u16 points to a
  record: a routine u16, then x and y in pixels. GP records add an i16 column step: kart k
  stands at x + (k % 2) × step, y + 24k. Battle records hold one `x y` per player.

### Graphics

- **Road tilesets**: track list group 0. A 256-byte header holding each tile's palette in its high
  nibble, then 32 bytes per tile of linear 4bpp, low nibble first; a pixel is palette × 16 +
  colour. Each Theme has up to 192 tiles of its own; streams that stop short leave the rest unused.
  Indices 192–255 are shared, from the stream at `0x40000`.
- **Palettes**: track list group 1. 512 bytes per Theme, 16 palettes × 16 BGR555 colours: all of
  CGRAM. 0–7 are for backgrounds and 8–15 for sprites.
- **Skies**: track list group 3 holds 2bpp planar tiles (`0x20000`–`0x214ED`), group 4 the layout:
  24 rows of 32 SNES tile map entries. Rows 0–15 are the near layer, four 32×4 screens side by
  side; rows 16–23 the far layer, two screens. Colours are Mode 0's: near uses CGRAM 64 + palette
  × 4 + colour, far 96 + palette × 4 + colour. Ghost Valley's Boos and Rainbow Road's stars are
  animated in game.
- **Drivers**: 4bpp planar, 32×32 frames, `0x6000` bytes per driver:

  | Driver | Offset |
  |---|---|
  | Mario | `0x2000` |
  | Bowser | `0x12000` |
  | Peach | `0x22000` |
  | DK Jr. | `0x32000` |
  | Yoshi | `0x42000` |
  | Koopa Troopa | `0x52000` |
  | Toad | `0x62000` |
  | Luigi | `0x4A000`–`0x4C7FF` |

  The palette index table is at `0x848E` and the angle table at `0x9090`; neither has been
  checked. Yoshi's bank differs between Europe and USA in 65 of its 768 tiles.
- **Other graphics**:

  | Asset | Offset |
  |---|---|
  | Items | `0x40594` |
  | Chain Chomp, read at the driver select screen | `0x60000` |
  | Lakitu, coin, shadow | `0x10000` |
  | Font | `0x70000` |
  | Title | `0x71996` |
  | Podium | `0x737DA` |

- **Sprite frames**: a frame record starts with a sprite routine in bank 0 (USA `0xBC0F`, Japan
  and Europe `0xBC34` for the first). By offset from it: `0x00`/`0x08` draw 2 16×16 sprites,
  `0x0D`/`0x15` 2 8×8, `0x6F`/`0x77` 4 16×16, `0x7C`/`0x84` 4 8×8, `0x147`–`0x157` one 16×16,
  `0x15F`–`0x16F` one 8×8; `0xAD4` draws nothing. 2- and 4-sprite records then hold a pointer to
  (y, x) i16 offsets and one to tile words (tile in bits 0–8, palette 9–11, h-flip 14, v-flip
  15); 1-sprite records hold the tile word. A 16×16 sprite is tiles t, t + 1, t + 16, t + 17.
- **Hazards**: a Theme's 57 tiles (track list group 2) load at OBJ tile `0xC0`. Each hazard has a
  script: two op words, then 12 frame pointers, one per size, of which the game uses 11. The game
  XORs a value into the tile words, which changes the palette:

  | Hazard | Graphics, palette | Scripts (USA; Europe +`0x25`) | XOR |
  |---|---|---|---|
  | Pipe | MC, MC (VL is the same) | `0xE582` | 0 |
  | Orange pipe (DP1) | MC, DP | `0xE582` | `0x400` |
  | Pillar | GV | `0xE716` (Japan `0xE71C`) | 0 |
  | Monty Mole, peeking and jumping | DP | `0xE3BC`, `0xE3D8` | 0 |
  | Piranha Plant, two frames | CI | `0xE0BC`, `0xE0D8` | 0 |
  | Cheep Cheep, two frames | KB | `0xE0BC`, `0xE0D8` | 0 |
  | Thwomp, and its shadow | BC | `0xE2C3`, `0xED2B` (Japan `0xED31`) | `0x600`, 0 |

  Japan's other scripts are at Europe's offsets. Rainbow Road's Thwomps are Bowser Castle's,
  cycling through all 8 palettes. Pillars pad frames with OBJ tile `0x84`, a blank 16×16 block.
- **Items**: `0x40594` holds 6 blocks of 640 bytes: banana, two shell spin frames, poison
  mushroom, fireball, egg. Each is four 16×16 frames of 4 consecutive tiles, then four 8×8
  frames; the game copies one into OBJ tile `0x160` per size. Palettes, from any Theme: banana,
  shell and fireball 6, red shell, mushroom and egg 5.
- About 28 code offsets differ between Regions; Epic Edit's `Offsets.cs` lists them, one table per
  Region. The data banks are mostly identical.

## Open research (milestone 7)

The decisions are all made; what's left is research:

- Driver frame order, angle mapping and palette selection, using Mesen.
- Where the Cast lives: Lakitu's props and the ending extras.
- Where the Effects live: dust, sparks, splashes, smoke, shadows.
- HUD pieces, the Screens list (title, select screens, podium, ending) and how each screen is built
  from tiles.
- Any Japan or Europe offsets still missing from the table above.
