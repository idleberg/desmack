use std::fs;
use std::path::Path;

use anyhow::{Context, ensure};

use crate::maps::{bytes, stream, word};
use crate::rom::{Region, Rom};
use crate::sprites::{DRIVERS, LAKITU_THEME, Sprite, draw, race_tiles};
use crate::textures::{colours, palette, write_png};

/// 256 glyphs of 2bpp planar tiles, which the game loads as OBJ tiles 0–255. Japan's has kana
/// where the others have more Latin letters.
const FONT: usize = 0x70000;
/// The menus draw text in sprite palette 4 of the title screen's CGRAM.
const FONT_PALETTE: usize = 128 + 4 * 16;

/// The title screen: 4bpp tiles and both tile maps as they go into VRAM, then all of CGRAM.
fn title(region: Region) -> usize {
	match region {
		Region::Usa | Region::Europe => 0x71996,
		Region::Japan => 0x71C48,
	}
}
const TITLE_CGRAM: usize = 0x4000;
/// The title's layers in VRAM: the logo on BG1, and the 512-pixel landscape that scrolls behind
/// it on BG2.
const TITLE_BG1: usize = 0x2000;
const TITLE_BG2: usize = 0x2800;

/// The menus' tiles, tile maps and palettes, which the game unpacks to `$7F:C000`.
fn menus(region: Region) -> usize {
	match region {
		Region::Usa | Region::Europe => 0x70B29,
		Region::Japan => 0x70D31,
	}
}
/// Driver select: its own tile maps for BG1–3 and its CGRAM, then the 2bpp tiles. BG4 is one
/// tile repeated, which the game writes itself; its BG2 scrolls the "Choose your driver" text.
const DRIVER_BG1: usize = 0x0000;
const DRIVER_BG2: usize = 0x0800;
const DRIVER_BG3: usize = 0x1800;
const DRIVER_CGRAM: usize = 0x2800;
const DRIVER_TILES: usize = 0x2A00;
const DRIVER_FILL: u16 = 0x3014;
/// Cup select: a BG1 tile map, its CGRAM and its own 2bpp tiles. The cup names are drawn over it
/// at run time.
const CUP_MAP: usize = 0x2F00;
const CUP_CGRAM: usize = 0x3700;
const CUP_TILES: usize = 0x3900;
fn cup_add(region: Region) -> u16 {
	match region {
		Region::Usa | Region::Europe => 0xF440,
		Region::Japan => 0x1040,
	}
}
/// The game copies 96 of the tiles, over a copy of the font from BG tile `0x40`.
const CUP_TILE_COUNT: usize = 96;
const CUP_FONT: usize = 0x40;

/// The visible height of a screen.
const HEIGHT: usize = 224;

/// The race HUD's 2bpp tiles, from BG tile `0x80`: timer digits, the player box and item icons.
/// The HUD is a bar on BG2 of the top screen, which is Mode 0, so its colours start at 32.
const HUD_TILES: usize = 0x112F8;
const HUD_FIRST: usize = 0x80;
const HUD_COLOURS: usize = 32;
/// The bar's palette and priority, which the game fills it with before writing tiles.
const BAR: u16 = 0x3C00;
/// The HUD's tables are in bank `$81`, which sits `0x144` bytes earlier in Japan's ROM and
/// `0x164` in Europe's.
fn hud_table(region: Region, usa: usize) -> usize {
	usa - match region {
		Region::Usa => 0,
		Region::Japan => 0x144,
		Region::Europe => 0x164,
	}
}
/// The bar's top line as a run-length list the game writes to VRAM, then each player's box's
/// lower two lines as a copy list, which go to column 18.
const BAR_TOP: usize = 0x1B826;
const BOXES: [(&str, usize); 2] = [("player-1", 0x1B836), ("player-2", 0x1B84A)];
const BOX_COLUMN: usize = 18;
/// An item's icon is a tile word, drawn with the next tile beside it and the two after below,
/// inside the box. Once the item is used, the box holds these two tile words instead.
const ICONS: usize = 0x1B320;
const ICON_NAMES: [&str; 11] = [
	"mushroom",
	"feather",
	"star",
	"banana",
	"green-shell",
	"red-shell",
	"boo",
	"coin",
	"lightning",
	"stolen",
	"unused",
];
const EMPTY: [u16; 2] = [0x3CE9, 0x3CE8];
/// A timer digit is tile `0x80` + d over `0x90` + d; the marks after minutes and seconds have
/// only a top half.
const TIMER_TOP: u16 = 0x80;
const TIMER_BOTTOM: u16 = 0x90;
const MINUTES: u16 = 0x8A;
const SECONDS: u16 = 0x8B;
const BLANK: u16 = 0x8F;

/// The rank is no tile: per line, HDMA sets both colour windows, and colour math adds grey inside
/// them. Each line is window 1's left and right, then window 2's; a window whose left is past its
/// right is off. First the digits 1–8 drawn during the race, then those drawn after the finish.
const RANKS: usize = 0x5E770;
const RANK_SETS: [(&str, usize, u8, usize); 2] = [("race", 20, 0xE0, 16), ("finish", 64, 0xC0, 64)];

/// The coin and lives counters: rows of sprites, each a count, an attribute byte, x and y, and
/// a tile per sprite, until a 0. The game patches digits into them from the digit table.
const COUNTERS: usize = 0x1BB5F;
const COUNTER_DIGITS: usize = 0x1BABF;

/// The finish ladder's 4bpp tiles in rows of 16: three rows of the drivers' 16×16 faces, in the
/// drivers' order, then the digits 1–8, each tile `0x60` + d over `0x70` + d. After the finish,
/// each rank shows its digit with the driver's face beside it, which flips to the happy face for
/// ranks 1–4 and the sad one for 5–8 every 8 frames. A digit is in sprite palette 1, or flashes
/// through palettes 1, 1, 5 and 6, two frames each.
const LADDER: usize = 0x30000;
const FACES: [&str; 3] = ["happy", "sad", "neutral"];
const LADDER_DIGIT: u16 = 0x60;
const LADDER_PALETTES: [(&str, u16); 3] = [("", 1), ("-red", 5), ("-orange", 6)];

pub fn font(rom: &Rom, folder: &Path) -> anyhow::Result<()> {
	let glyphs = stream(&rom.data, FONT)?;
	ensure!(glyphs.len() == 256 * 16, "the font is not 256 glyphs");
	let palette = colours(bytes(
		&stream(&rom.data, title(rom.region))?,
		TITLE_CGRAM + FONT_PALETTE * 2,
		8,
	)?);
	let mut pixels = vec![0; 128 * 128];
	for glyph in 0..256 {
		for y in 0..8 {
			let planes = &glyphs[glyph * 16 + y * 2..];
			for x in 0..8 {
				pixels[(glyph / 16 * 8 + y) * 128 + glyph % 16 * 8 + x] =
					(planes[0] >> (7 - x) & 1) | (planes[1] >> (7 - x) & 1) << 1;
			}
		}
	}
	let dir = folder.join("graphics/font");
	fs::create_dir_all(&dir)?;
	write_png(&dir.join("font.png"), 128, 128, &palette, &pixels)
}

pub fn screens(rom: &Rom, folder: &Path) -> anyhow::Result<()> {
	let dir = folder.join("graphics/screens");
	let vram = stream(&rom.data, title(rom.region))?;
	write_screen(
		&dir,
		"title",
		&colours(bytes(&vram, TITLE_CGRAM, 512)?),
		&[
			layer(&vram, &vram[TITLE_BG1..], 1, 4, 0)?,
			layer(&vram, &vram[TITLE_BG2..], 2, 4, 0)?,
		],
	)?;

	let menus = stream(&rom.data, menus(rom.region))?;
	ensure!(menus.len() == 0x4000, "the menu graphics are not 16 KB");
	// Mode 0: 2bpp tiles, and each BG has its own 32 colours.
	let tiles = &menus[DRIVER_TILES..CUP_MAP];
	let fill = [DRIVER_FILL.to_le_bytes(); 32 * 32].concat();
	write_screen(
		&dir,
		"driver-select",
		&colours(&menus[DRIVER_CGRAM..DRIVER_TILES]),
		&[
			layer(tiles, &menus[DRIVER_BG1..], 1, 2, 0)?,
			layer(tiles, &menus[DRIVER_BG2..], 1, 2, 32)?,
			layer(tiles, &menus[DRIVER_BG3..], 1, 2, 64)?,
			layer(tiles, &fill, 1, 2, 96)?,
		],
	)?;

	// Mode 1: the game pads 2bpp tiles to 4bpp, and adds a constant to every map entry.
	let font = stream(&rom.data, FONT)?;
	let mut tiles = vec![0; 0x400 * 32];
	for (tile, planes) in font.chunks(16).enumerate() {
		tiles[(CUP_FONT + tile) * 32..][..16].copy_from_slice(planes);
	}
	for (tile, planes) in menus[CUP_TILES..CUP_TILES + CUP_TILE_COUNT * 16]
		.chunks(16)
		.enumerate()
	{
		tiles[tile * 32..][..16].copy_from_slice(planes);
	}
	let map: Vec<u8> = menus[CUP_MAP..CUP_CGRAM]
		.chunks(2)
		.flat_map(|e| {
			(u16::from_le_bytes([e[0], e[1]]).wrapping_add(cup_add(rom.region))).to_le_bytes()
		})
		.collect();
	write_screen(
		&dir,
		"cup-select",
		&colours(&menus[CUP_CGRAM..CUP_TILES]),
		&[layer(&tiles, &map, 1, 4, 0)?],
	)
}

pub fn hud(rom: &Rom, folder: &Path) -> anyhow::Result<()> {
	let dir = folder.join("graphics/hud");
	let palette = palette(rom, LAKITU_THEME)?;
	let mut tiles = vec![0; 0x100 * 16];
	let hud = stream(&rom.data, HUD_TILES)?;
	ensure!(hud.len() <= 0x80 * 16, "the HUD tiles run long");
	tiles[HUD_FIRST * 16..][..hud.len()].copy_from_slice(&hud);
	let at = |usa| hud_table(rom.region, usa);

	// The player box, as the game draws it, then with each item and empty.
	let top = run_length(&rom.data, at(BAR_TOP))?;
	let top = bytes_to_words(&top)?;
	ensure!(
		top.len() >= BOX_COLUMN + 4,
		"the bar's top line stops short"
	);
	let top = top[BOX_COLUMN..BOX_COLUMN + 4].to_vec();
	fs::create_dir_all(dir.join("box"))?;
	let mut player = Vec::new();
	for (name, list) in BOXES {
		let lines = copy_list(&rom.data, at(list))?;
		ensure!(lines.len() >= 64 + 8, "{name}'s box stops short");
		let lower = [lines[..8].to_vec(), lines[64..72].to_vec()].map(|l| bytes_to_words(&l));
		let [middle, bottom] = lower;
		let rows = vec![top.clone(), middle?, bottom?];
		write_block(
			&dir.join(format!("box/{name}.png")),
			&tiles,
			&rows,
			&palette,
		)?;
		player.push(rows);
	}
	let with = |inner: [u16; 4]| {
		let mut rows = player[0].clone();
		rows[1][1..3].copy_from_slice(&inner[..2]);
		rows[2][1..3].copy_from_slice(&inner[2..]);
		rows
	};
	for (k, name) in ICON_NAMES.into_iter().enumerate() {
		let e = word(&rom.data, at(ICONS) + k * 2)? as u16;
		let rows = with([e, e + 1, e + 2, e + 3]);
		write_block(
			&dir.join(format!("box/{name}.png")),
			&tiles,
			&rows,
			&palette,
		)?;
	}
	let [upper, lower] = EMPTY;
	let rows = with([upper, upper, lower, lower]);
	write_block(&dir.join("box/empty.png"), &tiles, &rows, &palette)?;

	// The timer as it starts, and its digits.
	fs::create_dir_all(dir.join("timer"))?;
	let digit = |d: u16| [BAR | TIMER_TOP | d, BAR | TIMER_BOTTOM | d];
	let [zero, under] = digit(0);
	let rows = vec![
		vec![
			zero,
			zero,
			BAR | MINUTES,
			zero,
			zero,
			BAR | SECONDS,
			zero,
			zero,
		],
		vec![
			under,
			under,
			BAR | BLANK,
			under,
			under,
			BAR | BLANK,
			under,
			under,
		],
	];
	write_block(&dir.join("timer.png"), &tiles, &rows, &palette)?;
	for d in 0..10 {
		let [upper, lower] = digit(d);
		let rows = vec![vec![upper], vec![lower]];
		write_block(&dir.join(format!("timer/{d}.png")), &tiles, &rows, &palette)?;
	}

	// The rank digits, white where the game brightens the screen.
	let ranks = stream(&rom.data, RANKS)?;
	let mut start = 0;
	for (set, lines, left, width) in RANK_SETS {
		fs::create_dir_all(dir.join("rank").join(set))?;
		for rank in 1..=8 {
			let digit = bytes(&ranks, start, lines * 4)?;
			let pixels: Vec<u8> = digit
				.chunks(4)
				.flat_map(|w| {
					(0..width).map(move |x| {
						let x = left.wrapping_add(x as u8);
						u8::from(w[0] <= x && x <= w[1] || w[2] <= x && x <= w[3])
					})
				})
				.collect();
			write_png(
				&dir.join(format!("rank/{set}/{rank}.png")),
				width as u32,
				lines as u32,
				&[0, 0, 0, 255, 255, 255],
				&pixels,
			)?;
			start += lines * 4;
		}
	}

	// The coin and lives counters as the game first draws them, and their digits.
	let race = race_tiles(rom)?;
	let mut sprites = Vec::new();
	let mut row = at(COUNTERS);
	loop {
		let [head] = bytes(&rom.data, row, 1)?.try_into()?;
		if head == 0 {
			break;
		}
		ensure!(
			head & 0x60 == 0,
			"a counter row steps down or uses 16×16 sprites"
		);
		let [attr, x, y] = bytes(&rom.data, row + 1, 3)?.try_into()?;
		let count = usize::from(head);
		for (i, &tile) in bytes(&rom.data, row + 4, count)?.iter().enumerate() {
			sprites.push(Sprite {
				x: i32::from(x) + i as i32 * 8,
				y: i32::from(y),
				tile: u16::from(tile) | u16::from(attr) << 8,
				big: false,
			});
		}
		row += 4 + count;
	}
	let attr = sprites
		.first()
		.context("the counters have no sprites")?
		.tile & 0xFF00;
	let (width, height, pixels) = draw(&sprites, &race)?;
	write_png(&dir.join("counters.png"), width, height, &palette, &pixels)?;
	fs::create_dir_all(dir.join("counters"))?;
	for (d, &tile) in bytes(&rom.data, at(COUNTER_DIGITS), 10)?.iter().enumerate() {
		let digit = Sprite {
			x: 0,
			y: 0,
			tile: u16::from(tile) | attr,
			big: false,
		};
		let (width, height, pixels) = draw(&[digit], &race)?;
		write_png(
			&dir.join(format!("counters/{d}.png")),
			width,
			height,
			&palette,
			&pixels,
		)?;
	}

	// The finish ladder: each driver's faces, and the rank digits.
	let ladder = stream(&rom.data, LADDER)?;
	ensure!(ladder.len() == 120 * 32, "the ladder is not 120 tiles");
	fs::create_dir_all(dir.join("ladder"))?;
	for (row, face) in FACES.into_iter().enumerate() {
		for (driver, (name, pal)) in DRIVERS.into_iter().enumerate() {
			let sprite = Sprite {
				x: 0,
				y: 0,
				tile: (row * 32 + driver * 2) as u16 | pal << 9,
				big: true,
			};
			let (width, height, pixels) = draw(&[sprite], &ladder)?;
			let path = dir.join(format!("ladder/{name}-{face}.png"));
			write_png(&path, width, height, &palette, &pixels)?;
		}
	}
	for d in 0..8 {
		for (suffix, pal) in LADDER_PALETTES {
			let sprite = |y, tile| Sprite {
				x: 0,
				y,
				tile: (tile + d) | pal << 9,
				big: false,
			};
			let digit = [sprite(0, LADDER_DIGIT), sprite(8, LADDER_DIGIT + 16)];
			let (width, height, pixels) = draw(&digit, &ladder)?;
			let path = dir.join(format!("ladder/{}{suffix}.png", d + 1));
			write_png(&path, width, height, &palette, &pixels)?;
		}
	}
	Ok(())
}

/// A list the game writes to VRAM: a byte n, then n / 2 words when n is even, or one word to
/// repeat n / 2 times when it is odd, until a 0. Returns the bytes written.
fn run_length(data: &[u8], mut at: usize) -> anyhow::Result<Vec<u8>> {
	let mut out = Vec::new();
	loop {
		let [n] = bytes(data, at, 1)?.try_into()?;
		at += 1;
		let count = usize::from(n / 2);
		match n {
			0 => return Ok(out),
			_ if n % 2 == 1 => {
				out.extend(bytes(data, at, 2)?.repeat(count));
				at += 2;
			}
			_ => {
				out.extend(bytes(data, at, count * 2)?);
				at += count * 2;
			}
		}
	}
}

/// A list the game copies into a tile map buffer: words, until one with the top bit set, whose low
/// byte skips that many bytes, or ends the list when 0. Returns the bytes written, skips as 0.
fn copy_list(data: &[u8], mut at: usize) -> anyhow::Result<Vec<u8>> {
	let mut out = Vec::new();
	loop {
		let w = word(data, at)?;
		at += 2;
		if w & 0x8000 == 0 {
			out.extend((w as u16).to_le_bytes());
		} else if w & 0xFF == 0 {
			return Ok(out);
		} else {
			out.resize(out.len() + (w & 0xFF), 0);
		}
	}
}

fn bytes_to_words(bytes: &[u8]) -> anyhow::Result<Vec<u16>> {
	ensure!(
		bytes.len().is_multiple_of(2),
		"a tile map line has an odd length"
	);
	Ok(bytes
		.chunks(2)
		.map(|b| u16::from_le_bytes([b[0], b[1]]))
		.collect())
}

/// Writes rows of HUD tile map entries as one image.
fn write_block(path: &Path, tiles: &[u8], rows: &[Vec<u16>], palette: &[u8]) -> anyhow::Result<()> {
	let width = rows[0].len() * 8;
	let mut pixels = vec![0; width * rows.len() * 8];
	for (r, row) in rows.iter().enumerate() {
		for (c, &e) in row.iter().enumerate() {
			for (i, p) in tile(tiles, e, 2, HUD_COLOURS)?.into_iter().enumerate() {
				pixels[(r * 8 + i / 8) * width + c * 8 + i % 8] = p;
			}
		}
	}
	write_png(
		path,
		width as u32,
		(rows.len() * 8) as u32,
		palette,
		&pixels,
	)
}

/// Writes each layer as `<name>/bg<n>.png`, BG1 first, and the screen as `<name>.png`: the layers'
/// first 256 columns stacked, BG1 in front. Tile priorities are left out.
fn write_screen(dir: &Path, name: &str, palette: &[u8], layers: &[Vec<u8>]) -> anyhow::Result<()> {
	fs::create_dir_all(dir.join(name))?;
	let mut screen = vec![0; 256 * HEIGHT];
	for (n, pixels) in layers.iter().enumerate().rev() {
		let width = pixels.len() / HEIGHT;
		write_png(
			&dir.join(name).join(format!("bg{}.png", n + 1)),
			width as u32,
			HEIGHT as u32,
			palette,
			pixels,
		)?;
		for (row, line) in screen.chunks_mut(256).zip(pixels.chunks(width)) {
			for (pixel, &over) in row.iter_mut().zip(line) {
				if over != 0 {
					*pixel = over;
				}
			}
		}
	}
	write_png(
		&dir.join(format!("{name}.png")),
		256,
		HEIGHT as u32,
		palette,
		&screen,
	)
}

/// A background layer of `screens` 32×32 tile maps side by side, cut to the visible height. Each
/// entry holds a tile (bits 0–9), a palette (10–12) and flips (14, 15); a pixel is `base` +
/// palette × colours per palette + colour.
fn layer(
	tiles: &[u8],
	map: &[u8],
	screens: usize,
	bpp: usize,
	base: usize,
) -> anyhow::Result<Vec<u8>> {
	let width = screens * 256;
	let mut pixels = vec![0; width * HEIGHT];
	for row in 0..HEIGHT / 8 {
		for column in 0..screens * 32 {
			let at = (column / 32 * 32 * 32 + row * 32 + column % 32) * 2;
			let e = bytes(map, at, 2)?;
			let e = u16::from(e[0]) | u16::from(e[1]) << 8;
			for (i, p) in tile(tiles, e, bpp, base)?.into_iter().enumerate() {
				if p != 0 {
					pixels[(row * 8 + i / 8) * width + column * 8 + i % 8] = p;
				}
			}
		}
	}
	Ok(pixels)
}

/// The 8×8 pixels of a tile map entry, as [`layer`] colours them; 0 is transparent.
fn tile(tiles: &[u8], e: u16, bpp: usize, base: usize) -> anyhow::Result<[u8; 64]> {
	let e = usize::from(e);
	let palette = base + ((e >> 10 & 7) << bpp);
	let mut pixels = [0; 64];
	for y in 0..8 {
		let line = if e & 0x8000 != 0 { 7 - y } else { y };
		let planes = bytes(
			tiles,
			(e & 0x3FF) * bpp * 8 + line * 2,
			(bpp / 2 - 1) * 16 + 2,
		)?;
		for x in 0..8 {
			let bit = if e & 0x4000 != 0 { x } else { 7 - x };
			let v = (0..bpp).fold(0, |v, plane| {
				v | (planes[plane / 2 * 16 + plane % 2] >> bit & 1) << plane
			});
			if v != 0 {
				pixels[y * 8 + x] = (palette | usize::from(v)) as u8;
			}
		}
	}
	Ok(pixels)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn layer_reads_4bpp_planes_palettes_and_flips() {
		// Tile 1: one colour-9 pixel at the top left, in planes 0 and 3.
		let mut tiles = vec![0; 64];
		tiles[32] = 0x80;
		tiles[32 + 17] = 0x80;
		// Two screens; the second's first entry is tile 1, palette 3, flipped both ways.
		let mut map = vec![0; 2 * 2048];
		map[2048..2050].copy_from_slice(&0xCC01_u16.to_le_bytes());
		let pixels = layer(&tiles, &map, 2, 4, 0).unwrap();
		assert_eq!(pixels[7 * 512 + 256 + 7], 3 * 16 + 9);
		assert_eq!(pixels.iter().filter(|&&p| p != 0).count(), 1);
	}

	#[test]
	fn run_length_and_copy_lists() {
		// Two literal words, then one repeated three times.
		let list = [0x04, 1, 0, 2, 0, 0x07, 3, 0, 0x00];
		assert_eq!(
			run_length(&list, 0).unwrap(),
			[1, 0, 2, 0, 3, 0, 3, 0, 3, 0]
		);
		// A word, a 2-byte skip, a word.
		let list = [1, 0, 0x02, 0x80, 2, 0, 0x00, 0x80];
		assert_eq!(copy_list(&list, 0).unwrap(), [1, 0, 0, 0, 2, 0]);
	}

	#[test]
	fn smk_rom_graphics() {
		let Ok(path) = std::env::var("SMK_ROM") else {
			return;
		};
		let rom = Rom::parse(fs::read(path).unwrap()).unwrap();
		let folder = std::env::temp_dir().join(format!("desmack-graphics-{}", std::process::id()));
		font(&rom, &folder).unwrap();
		screens(&rom, &folder).unwrap();
		hud(&rom, &folder).unwrap();
		let graphics = folder.join("graphics");
		let size = |path: &str| {
			let reader = png::Decoder::new(fs::File::open(graphics.join(path)).unwrap())
				.read_info()
				.unwrap();
			(reader.info().width, reader.info().height)
		};
		assert_eq!(size("font/font.png"), (128, 128));
		assert_eq!(size("screens/title.png"), (256, 224));
		assert_eq!(size("screens/title/bg2.png"), (512, 224));
		assert_eq!(size("screens/driver-select/bg4.png"), (256, 224));
		assert_eq!(size("screens/cup-select.png"), (256, 224));
		assert_eq!(size("hud/box/star.png"), (32, 24));
		assert_eq!(size("hud/timer.png"), (64, 16));
		assert_eq!(size("hud/rank/race/8.png"), (16, 20));
		assert_eq!(size("hud/rank/finish/1.png"), (64, 64));
		assert_eq!(size("hud/counters.png"), (40, 16));
		assert_eq!(size("hud/ladder/yoshi-sad.png"), (16, 16));
		assert_eq!(size("hud/ladder/8.png"), (8, 16));
		fs::remove_dir_all(&folder).unwrap();
	}
}
