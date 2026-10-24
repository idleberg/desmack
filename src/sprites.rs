use std::fs;
use std::path::Path;

use anyhow::{Context, bail, ensure};

use crate::maps::{bytes, stream, word};
use crate::rom::{Region, Rom};
use crate::textures::{palette, theme_stream, write_png};

/// Six blocks of 640 bytes: four 16×16 frames of 4 tiles each, then four 8×8 frames.
const ITEMS: usize = 0x40594;

/// Each item's blocks and sprite palette. The two shell blocks are its spin; the red shell is the
/// green one in another palette.
const ITEM_SETS: [(&str, &[usize], u16); 6] = [
	("banana", &[0], 6),
	("green-shell", &[1, 2], 6),
	("red-shell", &[1, 2], 5),
	("poison-mushroom", &[3], 5),
	("fireball", &[4], 6),
	("egg", &[5], 5),
];

/// Track list group 2: a Theme's 57 hazard tiles, which the game loads at OBJ tile `0xC0`.
const HAZARD_TILES: usize = 2;
const HAZARD_BASE: usize = 0xC0;
/// A blank 16×16 OBJ tile that Ghost Valley's pillars pad their frames with.
const BLANK: u16 = 0x84;

const GHOST_VALLEY: usize = 0;
const MARIO_CIRCUIT: usize = 1;
const DONUT_PLAINS: usize = 2;
const CHOCO_ISLAND: usize = 3;
const KOOPA_BEACH: usize = 5;
const BOWSER_CASTLE: usize = 6;

/// Indices into [`Scripts::of`]'s list.
const PIRANHA: usize = 0;
const THWOMP: usize = 2;
const MOLE: usize = 3;
const PIPE: usize = 5;
const PILLAR: usize = 6;
const SHADOW: usize = 7;

/// Each hazard's name, graphics Theme, palette Theme and scripts, each with the value the game
/// XORs into its tile words. Donut Plains 1 draws Mario Circuit's pipes in palette 5; Vanilla
/// Lake's pipes are Mario Circuit's; Rainbow Road's Thwomps are Bowser Castle's, cycling through
/// palettes.
type Scripted = &'static [(usize, u16)];

const HAZARDS: [(&str, usize, usize, Scripted); 7] = [
	("pipe", MARIO_CIRCUIT, MARIO_CIRCUIT, &[(PIPE, 0)]),
	("orange-pipe", MARIO_CIRCUIT, DONUT_PLAINS, &[(PIPE, 0x400)]),
	("pillar", GHOST_VALLEY, GHOST_VALLEY, &[(PILLAR, 0)]),
	(
		"monty-mole",
		DONUT_PLAINS,
		DONUT_PLAINS,
		&[(MOLE, 0), (MOLE + 1, 0)],
	),
	(
		"piranha-plant",
		CHOCO_ISLAND,
		CHOCO_ISLAND,
		&[(PIRANHA, 0), (PIRANHA + 1, 0)],
	),
	(
		"cheep-cheep",
		KOOPA_BEACH,
		KOOPA_BEACH,
		&[(PIRANHA, 0), (PIRANHA + 1, 0)],
	),
	(
		"thwomp",
		BOWSER_CASTLE,
		BOWSER_CASTLE,
		&[(THWOMP, 0x600), (SHADOW, 0)],
	),
];

/// Where each Region keeps its sprite routines and hazard scripts, in bank 0.
struct Scripts {
	routines: usize,
	scripts: [usize; 8],
}

impl Scripts {
	fn of(region: Region) -> Scripts {
		match region {
			Region::Japan => Scripts {
				routines: 0xBC34,
				scripts: [
					0xE0E1, 0xE0FD, 0xE2E8, 0xE3E1, 0xE3FD, 0xE5A7, 0xE71C, 0xED31,
				],
			},
			Region::Usa => Scripts {
				routines: 0xBC0F,
				scripts: [
					0xE0BC, 0xE0D8, 0xE2C3, 0xE3BC, 0xE3D8, 0xE582, 0xE716, 0xED2B,
				],
			},
			Region::Europe => Scripts {
				routines: 0xBC34,
				scripts: [
					0xE0E1, 0xE0FD, 0xE2E8, 0xE3E1, 0xE3FD, 0xE5A7, 0xE73B, 0xED50,
				],
			},
		}
	}
}

/// One OAM entry: a position, a tile word (tile, palette, flips) and whether it is 16×16.
#[derive(Clone, Copy)]
struct Sprite {
	x: i32,
	y: i32,
	tile: u16,
	big: bool,
}

pub fn items(rom: &Rom, folder: &Path) -> anyhow::Result<()> {
	let tiles = stream(&rom.data, ITEMS)?;
	ensure!(tiles.len() == 6 * 640, "item graphics are not 6 items");
	let palette = palette(rom, MARIO_CIRCUIT)?;
	for (name, blocks, pal) in ITEM_SETS {
		let mut frames = Vec::new();
		for block in blocks {
			for k in 0..8 {
				let tile = (block * 640 / 32 + if k < 4 { k * 4 } else { 16 + k - 4 }) as u16;
				let sprite = |x, y, tile| Sprite {
					x,
					y,
					tile: tile | pal << 9,
					big: false,
				};
				frames.push(if k < 4 {
					vec![
						sprite(0, 0, tile),
						sprite(8, 0, tile + 1),
						sprite(0, 8, tile + 2),
						sprite(8, 8, tile + 3),
					]
				} else {
					vec![sprite(0, 0, tile)]
				});
			}
		}
		write_frames(
			&folder.join("sprites/items").join(name),
			&frames,
			&tiles,
			&palette,
		)?;
	}
	Ok(())
}

pub fn hazards(rom: &Rom, folder: &Path) -> anyhow::Result<()> {
	let Scripts { routines, scripts } = Scripts::of(rom.region);
	for (name, graphics, colours, uses) in HAZARDS {
		let mut frames = Vec::new();
		for &(script, xor) in uses {
			frames.extend(script_frames(&rom.data, routines, scripts[script], xor)?);
		}
		write_frames(
			&folder.join("sprites/hazards").join(name),
			&frames,
			&theme_stream(rom, HAZARD_TILES, graphics)?,
			&palette(rom, colours)?,
		)?;
	}
	Ok(())
}

/// A hazard script is two op words, then frame records for 12 sizes, of which the game picks
/// from the first 11. A size that repeats the one before is the same frame.
fn script_frames(
	data: &[u8],
	routines: usize,
	script: usize,
	xor: u16,
) -> anyhow::Result<Vec<Vec<Sprite>>> {
	let mut frames = Vec::new();
	let mut last = None;
	for size in 0..11 {
		let record = word(data, script + 4 + size * 2)?;
		if last.replace(record) == Some(record) {
			continue;
		}
		let mut sprites = frame(data, routines, record)?;
		sprites.retain(|s| s.tile & 0x1FF != BLANK);
		for sprite in &mut sprites {
			let tile = usize::from(sprite.tile & 0x1FF)
				.checked_sub(HAZARD_BASE)
				.with_context(|| {
					format!("frame at {record:#X} uses a tile outside the hazard's")
				})?;
			sprite.tile = (sprite.tile & !0x1FF | tile as u16) ^ xor;
		}
		if !sprites.is_empty() {
			frames.push(sprites);
		}
	}
	Ok(frames)
}

/// A frame record starts with its sprite routine. Routines with 2 or 4 sprites point to their
/// (y, x) offsets and tile words; those with 1 sprite hold its tile word. Positions only matter
/// within a frame, so the routines' anchors and y adjustments are left out.
fn frame(data: &[u8], routines: usize, record: usize) -> anyhow::Result<Vec<Sprite>> {
	let routine = word(data, record)?;
	let (count, big) = match routine.wrapping_sub(routines) {
		0x00 | 0x08 => (2, true),
		0x0D | 0x15 => (2, false),
		0x6F | 0x77 => (4, true),
		0x7C | 0x84 => (4, false),
		0x147 | 0x14F | 0x157 => (1, true),
		0x15F | 0x167 | 0x16F => (1, false),
		// The script op that draws nothing.
		0xAD4 => return Ok(Vec::new()),
		_ => bail!("frame at {record:#X} uses unknown routine {routine:#X}"),
	};
	if count == 1 {
		let tile = word(data, record + 2)? as u16;
		return Ok(vec![Sprite {
			x: 0,
			y: 0,
			tile,
			big,
		}]);
	}
	let (offsets, tiles) = (word(data, record + 2)?, word(data, record + 4)?);
	(0..count)
		.map(|i| {
			Ok(Sprite {
				x: i32::from(word(data, offsets + i * 4 + 2)? as i16),
				y: i32::from(word(data, offsets + i * 4)? as i16),
				tile: word(data, tiles + i * 2)? as u16,
				big,
			})
		})
		.collect()
}

fn write_frames(
	dir: &Path,
	frames: &[Vec<Sprite>],
	tiles: &[u8],
	palette: &[u8],
) -> anyhow::Result<()> {
	fs::create_dir_all(dir)?;
	for (i, sprites) in frames.iter().enumerate() {
		let (width, height, pixels) = draw(sprites, tiles)?;
		write_png(
			&dir.join(format!("{i:03}.png")),
			width,
			height,
			palette,
			&pixels,
		)?;
	}
	Ok(())
}

/// Draws sprites from 4bpp planar OBJ tiles, cropped to their bounds. A 16×16 sprite is tiles
/// t, t + 1, t + 16 and t + 17, and the first sprite drawn wins, as in OAM. Colours are CGRAM's
/// sprite half: 128 + palette × 16 + colour.
fn draw(sprites: &[Sprite], tiles: &[u8]) -> anyhow::Result<(u32, u32, Vec<u8>)> {
	let size = |s: &Sprite| if s.big { 16 } else { 8 };
	let left = sprites.iter().map(|s| s.x).min().unwrap_or(0);
	let top = sprites.iter().map(|s| s.y).min().unwrap_or(0);
	let width = sprites.iter().map(|s| s.x + size(s)).max().unwrap_or(0) - left;
	let height = sprites.iter().map(|s| s.y + size(s)).max().unwrap_or(0) - top;
	let mut pixels = vec![0; (width * height) as usize];
	for s in sprites {
		let n = size(s);
		let colour = 128 + (s.tile >> 9 & 7) as u8 * 16;
		for y in 0..n {
			for x in 0..n {
				let sx = if s.tile & 0x4000 != 0 { n - 1 - x } else { x };
				let sy = if s.tile & 0x8000 != 0 { n - 1 - y } else { y };
				let tile = usize::from(s.tile & 0x1FF) + (sx / 8 + sy / 8 * 16) as usize;
				let row = bytes(tiles, tile * 32 + (sy % 8) as usize * 2, 18)?;
				let bit = 7 - sx % 8;
				let v = (row[0] >> bit & 1)
					| (row[1] >> bit & 1) << 1
					| (row[16] >> bit & 1) << 2
					| (row[17] >> bit & 1) << 3;
				let at = ((s.y - top + y) * width + s.x - left + x) as usize;
				if v != 0 && pixels[at] == 0 {
					pixels[at] = colour + v;
				}
			}
		}
	}
	Ok((width as u32, height as u32, pixels))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn draw_flips_crops_and_keeps_the_first_sprite() {
		// Tile 0: one colour-1 pixel at the top left. Tile 1: a colour-15 pixel there.
		let mut tiles = vec![0; 64];
		tiles[0] = 0x80;
		tiles[32..34].copy_from_slice(&[0x80, 0x80]);
		tiles[48..50].copy_from_slice(&[0x80, 0x80]);
		let sprites = [
			// Palette 2, flipped both ways, at (-8, -8).
			Sprite {
				x: -8,
				y: -8,
				tile: 0xC400,
				big: false,
			},
			Sprite {
				x: -8,
				y: -8,
				tile: 0x0001,
				big: false,
			},
			Sprite {
				x: 0,
				y: 0,
				tile: 0x0001,
				big: false,
			},
		];
		let (width, height, pixels) = draw(&sprites, &tiles).unwrap();
		assert_eq!((width, height), (16, 16));
		assert_eq!(pixels[7 * 16 + 7], 128 + 32 + 1);
		assert_eq!(pixels[0], 128 + 15);
		assert_eq!(pixels[8 * 16 + 8], 128 + 15);
		assert_eq!(pixels.iter().filter(|&&p| p != 0).count(), 3);
	}

	#[test]
	fn smk_rom_sprites() {
		let Ok(path) = std::env::var("SMK_ROM") else {
			return;
		};
		let rom = Rom::parse(fs::read(path).unwrap()).unwrap();
		let folder = std::env::temp_dir().join(format!("desmack-sprites-{}", std::process::id()));
		items(&rom, &folder).unwrap();
		hazards(&rom, &folder).unwrap();
		let count = |dir: &str| {
			fs::read_dir(folder.join("sprites").join(dir))
				.unwrap()
				.count()
		};
		assert_eq!(count("items/green-shell"), 16);
		assert_eq!(count("hazards/pipe"), 11);
		assert_eq!(count("hazards/piranha-plant"), 20);
		assert_eq!(count("hazards/thwomp"), 11 + 5);
		fs::remove_dir_all(&folder).unwrap();
	}
}
