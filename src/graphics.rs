use std::fs;
use std::path::Path;

use anyhow::ensure;

use crate::maps::{bytes, stream};
use crate::rom::{Region, Rom};
use crate::textures::{colours, write_png};

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
			let e = usize::from(e[0]) | usize::from(e[1]) << 8;
			let palette = base + ((e >> 10 & 7) << bpp);
			for y in 0..8 {
				let line = if e & 0x8000 != 0 { 7 - y } else { y };
				let tile = bytes(
					tiles,
					(e & 0x3FF) * bpp * 8 + line * 2,
					(bpp / 2 - 1) * 16 + 2,
				)?;
				for x in 0..8 {
					let bit = if e & 0x4000 != 0 { x } else { 7 - x };
					let v = (0..bpp).fold(0, |v, plane| {
						v | (tile[plane / 2 * 16 + plane % 2] >> bit & 1) << plane
					});
					if v != 0 {
						pixels[(row * 8 + y) * width + column * 8 + x] =
							(palette | usize::from(v)) as u8;
					}
				}
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
	fn smk_rom_graphics() {
		let Ok(path) = std::env::var("SMK_ROM") else {
			return;
		};
		let rom = Rom::parse(fs::read(path).unwrap()).unwrap();
		let folder = std::env::temp_dir().join(format!("desmack-graphics-{}", std::process::id()));
		font(&rom, &folder).unwrap();
		screens(&rom, &folder).unwrap();
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
		fs::remove_dir_all(&folder).unwrap();
	}
}
