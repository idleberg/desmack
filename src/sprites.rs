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

/// Indices into [`Bank0::of`]'s scripts.
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

/// Where each Region keeps its sprite routines, hazard scripts and driver tables, in bank 0.
struct Bank0 {
	routines: usize,
	scripts: [usize; 8],
	drivers: usize,
}

impl Bank0 {
	fn of(region: Region) -> Bank0 {
		match region {
			Region::Japan => Bank0 {
				routines: 0xBC34,
				scripts: [
					0xE0E1, 0xE0FD, 0xE2E8, 0xE3E1, 0xE3FD, 0xE5A7, 0xE71C, 0xED31,
				],
				drivers: 0x9069,
			},
			Region::Usa => Bank0 {
				routines: 0xBC0F,
				scripts: [
					0xE0BC, 0xE0D8, 0xE2C3, 0xE3BC, 0xE3D8, 0xE582, 0xE716, 0xED2B,
				],
				drivers: 0x9090,
			},
			Region::Europe => Bank0 {
				routines: 0xBC34,
				scripts: [
					0xE0E1, 0xE0FD, 0xE2E8, 0xE3E1, 0xE3FD, 0xE5A7, 0xE73B, 0xED50,
				],
				drivers: 0x9077,
			},
		}
	}
}

/// The drivers in the game's order, which the bank table follows, with their sprite palettes: two
/// drivers share each of sprite palettes 8–11.
const DRIVERS: [(&str, u16); 8] = [
	("mario", 1),
	("luigi", 2),
	("bowser", 0),
	("peach", 3),
	("dk-jr", 3),
	("koopa-troopa", 2),
	("toad", 1),
	("yoshi", 0),
];
const LUIGI: usize = 1;

/// The driver tables, from [`Bank0::of`]'s `drivers`: a (frame, layout) byte pair per frame code;
/// then each frame's address in a driver's bank; then, for Luigi, a word per frame code that picks
/// his own upper half, from the bank table after it. The banks come 16 bytes before.
const CODES: usize = 181;
const FRAMES: usize = 76;
const FRAME_TABLE: usize = 0x16A;
const LUIGI_TABLE: usize = 0x202;
const LUIGI_TOPS: usize = 0x36C;
/// Luigi's upper halves are in bank `$84`; the rest of him is Mario's.
const LUIGI_BANK: usize = 0x84;
const MARIO_BANK: usize = 0xC0;

/// Lakitu, coins and the race's shared sprites: stream tiles 16–143 load at OBJ tile `0x40`, and
/// 12–15, the start lights, at `0xFB`.
const RACE_SPRITES: usize = 0x10000;
/// Lakitu's frames: a table of offsets from itself to metasprite records.
const LAKITU: usize = 0x5E264;
const LAKITU_FRAMES: usize = 14;
/// Lakitu's palette and his lights' are the same in every Theme.
const LAKITU_THEME: usize = MARIO_CIRCUIT;

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
	let Bank0 {
		routines, scripts, ..
	} = Bank0::of(rom.region);
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

pub fn cast(rom: &Rom, folder: &Path) -> anyhow::Result<()> {
	let stream = stream(&rom.data, RACE_SPRITES)?;
	ensure!(stream.len() >= 144 * 32, "race sprites stop short");
	let mut tiles = vec![0; 0x100 * 32];
	tiles[0x40 * 32..0xC0 * 32].copy_from_slice(&stream[16 * 32..144 * 32]);
	tiles[0xFB * 32..0xFF * 32].copy_from_slice(&stream[12 * 32..16 * 32]);
	let frames = (0..LAKITU_FRAMES)
		.map(|k| metasprite(&rom.data, LAKITU + word(&rom.data, LAKITU + k * 2)?))
		.collect::<anyhow::Result<Vec<_>>>()?;
	write_frames(
		&folder.join("sprites/cast/lakitu"),
		&frames,
		&tiles,
		&palette(rom, LAKITU_THEME)?,
	)
}

/// A metasprite record: a sprite count, then x and y as i8, the tile and the OAM attribute byte for
/// each sprite, then OAM's 2 high bits per sprite, of which the upper is the size.
fn metasprite(data: &[u8], at: usize) -> anyhow::Result<Vec<Sprite>> {
	let count = usize::from(bytes(data, at, 1)?[0]);
	let sizes = bytes(data, at + 1 + count * 4, count.div_ceil(4))?;
	(0..count)
		.map(|i| {
			let [x, y, tile, attr] = bytes(data, at + 1 + i * 4, 4)?.try_into()?;
			Ok(Sprite {
				x: i32::from(x as i8),
				y: i32::from(y as i8),
				tile: u16::from(tile) | u16::from(attr) << 8,
				big: sizes[i / 4] >> (i % 4 * 2 + 1) & 1 != 0,
			})
		})
		.collect()
}

/// Each driver's 76 frames, in frame table order. Two more frame codes draw from RAM the game fills
/// at run time, and are left out.
pub fn drivers(rom: &Rom, folder: &Path) -> anyhow::Result<()> {
	let codes = Bank0::of(rom.region).drivers;
	let palette = palette(rom, MARIO_CIRCUIT)?;
	for (driver, (name, pal)) in DRIVERS.into_iter().enumerate() {
		let dir = folder.join("sprites/drivers").join(name);
		fs::create_dir_all(&dir)?;
		for frame in 0..FRAMES {
			let code = (0..CODES)
				.find(|c| usize::from(rom.data[codes + c * 2]) == frame * 2)
				.with_context(|| format!("no frame code draws driver frame {frame}"))?;
			let (tiles, layout) = slot(&rom.data, codes, driver, code)?;
			let mut sprites = sprites_of(layout)?;
			for sprite in &mut sprites {
				sprite.tile |= pal << 9;
			}
			let (width, height, pixels) = draw(&sprites, &tiles)?;
			write_png(
				&dir.join(format!("{frame:03}.png")),
				width,
				height,
				&palette,
				&pixels,
			)?;
		}
	}
	Ok(())
}

/// A driver's VRAM slot, 4 rows of 16 tiles, as the game fills it for a frame code: the layout's
/// low nibble counts rows, its high nibble is the bytes per row, and rows are `0x200` apart in the
/// bank. Luigi's 32×32 frames take their upper two rows from his own bank and the lower two from
/// Mario's; a flag word picks half-width rows (`0x200`) or upper rows only (`0x400`). The
/// half-width flag also marks every mirrored 32×32 frame, which the layout byte misses for two of
/// them, so the slot comes with the layout to draw it with.
fn slot(data: &[u8], codes: usize, driver: usize, code: usize) -> anyhow::Result<(Vec<u8>, u8)> {
	let frame = word(
		data,
		codes + FRAME_TABLE + usize::from(data[codes + code * 2]),
	)?;
	let mut layout = data[codes + code * 2 + 1];
	let luigi = word(data, codes + LUIGI_TABLE + code * 2)?;
	if luigi & 0x8200 == 0x200 {
		layout = 0x44;
	}
	let at = |bank: usize, address: usize| (bank & 0x3F) << 16 | address;
	let rows: Vec<(usize, usize)> = if driver == LUIGI && luigi & 0x8000 == 0 {
		let top = word(data, codes + LUIGI_TOPS + (luigi & 0xFF))?;
		let size = if luigi & 0x200 != 0 { 0x40 } else { 0x80 };
		let mut rows = vec![
			(at(LUIGI_BANK, top), size),
			(at(LUIGI_BANK, top + 0x200), size),
		];
		if luigi & 0x400 == 0 {
			rows.push((at(MARIO_BANK, frame + 0x400), size));
			rows.push((at(MARIO_BANK, frame + 0x600), size));
		}
		rows
	} else {
		let bank = word(
			data,
			codes - 16 + if driver == LUIGI { 0 } else { driver * 2 },
		)?;
		(0..usize::from(layout & 15))
			.map(|r| (at(bank, frame + r * 0x200), usize::from(layout & 0xF0)))
			.collect()
	};
	let mut slot = vec![0; 4 * 0x200];
	for (r, (source, size)) in rows.into_iter().enumerate() {
		slot[r * 0x200..r * 0x200 + size].copy_from_slice(bytes(data, source, size)?);
	}
	Ok((slot, layout))
}

/// How the game places a layout's sprites over the slot. Symmetric frames hold a half, drawn
/// again mirrored; the 24-pixel ones overlap it, 7 pixels to the right. The `0x41` frames stack
/// their two tiles.
fn sprites_of(layout: u8) -> anyhow::Result<Vec<Sprite>> {
	let s = |x, y, tile, big| Sprite { x, y, tile, big };
	Ok(match layout {
		0x84 => vec![
			s(0, 0, 0, true),
			s(16, 0, 2, true),
			s(0, 16, 32, true),
			s(16, 16, 34, true),
		],
		0x82 => vec![s(0, 0, 0, true), s(16, 0, 2, true)],
		0x44 => vec![
			s(0, 0, 0, true),
			s(16, 0, 0x4000, true),
			s(0, 16, 32, true),
			s(16, 16, 0x4000 | 32, true),
		],
		0x63 => vec![
			s(0, 0, 0, true),
			s(8, 0, 1, true),
			s(0, 8, 16, true),
			s(8, 8, 17, true),
		],
		0x43 => vec![
			s(0, 0, 0, true),
			s(7, 0, 0x4000, true),
			s(0, 8, 16, true),
			s(7, 8, 0x4000 | 16, true),
		],
		0x42 => vec![s(0, 0, 0, true)],
		0x41 => vec![s(0, 0, 0, false), s(0, 8, 1, false)],
		0x21 => vec![s(0, 0, 0, false)],
		_ => bail!("unknown driver layout {layout:#X}"),
	})
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
		drivers(&rom, &folder).unwrap();
		items(&rom, &folder).unwrap();
		hazards(&rom, &folder).unwrap();
		cast(&rom, &folder).unwrap();
		let count = |dir: &str| {
			fs::read_dir(folder.join("sprites").join(dir))
				.unwrap()
				.count()
		};
		assert_eq!(count("drivers/luigi"), FRAMES);
		assert_eq!(count("cast/lakitu"), LAKITU_FRAMES);
		assert_eq!(count("items/green-shell"), 16);
		assert_eq!(count("hazards/pipe"), 11);
		assert_eq!(count("hazards/piranha-plant"), 20);
		assert_eq!(count("hazards/thwomp"), 11 + 5);
		fs::remove_dir_all(&folder).unwrap();
	}
}
