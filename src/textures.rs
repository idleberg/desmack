use std::fs;
use std::io::BufWriter;
use std::path::Path;

use anyhow::ensure;

use crate::maps::{Offsets, THEMES, bytes, long, stream};
use crate::rom::Rom;

/// Tile indices 192–255, which every Theme shares.
const COMMON_TILES: usize = 0x40000;

/// The track list's per-Theme pointer groups, after its 24 map pointers.
const TILES: usize = 0;
const PALETTE: usize = 1;
const SKY_TILES: usize = 3;
const SKY_LAYOUT: usize = 4;

fn theme_stream(rom: &Rom, group: usize, theme: usize) -> anyhow::Result<Vec<u8>> {
	let list = long(&rom.data, Offsets::of(rom.region).track_list)?;
	stream(
		&rom.data,
		long(&rom.data, list + 72 + group * 24 + theme * 3)?,
	)
}

/// The Theme's 256 BGR555 colours as RGB, which is all of CGRAM.
fn palette(rom: &Rom, theme: usize) -> anyhow::Result<Vec<u8>> {
	let palette = theme_stream(rom, PALETTE, theme)?;
	ensure!(
		palette.len() == 512,
		"{} palette is not 256 colours",
		THEMES[theme]
	);
	Ok(colours(&palette))
}

fn colours(bgr555: &[u8]) -> Vec<u8> {
	bgr555
		.chunks(2)
		.flat_map(|b| {
			let w = u16::from(b[0]) | u16::from(b[1]) << 8;
			[0, 5, 10].map(|shift| {
				let c = (w >> shift & 31) as u8;
				c << 3 | c >> 2
			})
		})
		.collect()
}

pub fn tilesets(rom: &Rom, folder: &Path) -> anyhow::Result<()> {
	let dir = folder.join("textures/tilesets");
	fs::create_dir_all(&dir)?;
	let common = stream(&rom.data, COMMON_TILES)?;
	for (theme, name) in THEMES.into_iter().enumerate() {
		let pixels = tileset(&theme_stream(rom, TILES, theme)?, &common);
		let path = dir.join(format!("{name}.png"));
		write_png(&path, 128, 128, &palette(rom, theme)?, &pixels)?;
	}
	Ok(())
}

/// Each stream is a 256-byte header, a tile's palette in the high nibble of its byte, then 32
/// bytes of linear 4bpp per tile, low nibble first. A Theme's stream may stop short of 192 tiles;
/// the rest are left empty, as no course uses them.
fn tileset(tiles: &[u8], common: &[u8]) -> Vec<u8> {
	let mut pixels = vec![0; 128 * 128];
	for index in 0..256 {
		let (src, i) = if index < 192 {
			(tiles, index)
		} else {
			(common, index - 192)
		};
		let palette = src.get(i).map_or(0, |b| b >> 4) << 4;
		for k in 0..64 {
			let Some(&b) = src.get(256 + i * 32 + k / 2) else {
				break;
			};
			let v = if k % 2 == 0 { b & 15 } else { b >> 4 };
			pixels[(index / 16 * 8 + k / 8) * 128 + index % 16 * 8 + k % 8] = palette | v;
		}
	}
	pixels
}

pub fn skies(rom: &Rom, folder: &Path) -> anyhow::Result<()> {
	let dir = folder.join("textures/skies");
	fs::create_dir_all(&dir)?;
	for (theme, name) in THEMES.into_iter().enumerate() {
		let tiles = theme_stream(rom, SKY_TILES, theme)?;
		let layout = theme_stream(rom, SKY_LAYOUT, theme)?;
		ensure!(layout.len() == 1536, "{name} sky is not 24 rows of 32");
		let palette = palette(rom, theme)?;
		let near = sky(&tiles, &layout, 0, 4, 64)?;
		write_png(
			&dir.join(format!("{name}-near.png")),
			1024,
			32,
			&palette,
			&near,
		)?;
		let far = sky(&tiles, &layout, 16, 2, 96)?;
		write_png(
			&dir.join(format!("{name}-far.png")),
			512,
			32,
			&palette,
			&far,
		)?;
	}
	Ok(())
}

/// A sky layer: `screens` tile maps of 32×4 entries side by side, from `row` of the layout. The
/// tiles are 2bpp planar, and the near and far layers use their own 32 colours from `base`, as
/// Mode 0 gives each background.
fn sky(
	tiles: &[u8],
	layout: &[u8],
	row: usize,
	screens: usize,
	base: u8,
) -> anyhow::Result<Vec<u8>> {
	let width = screens * 256;
	let mut pixels = vec![0; width * 32];
	for s in 0..screens {
		for r in 0..4 {
			for c in 0..32 {
				let e = bytes(layout, ((row + s * 4 + r) * 32 + c) * 2, 2)?;
				let e = u16::from(e[0]) | u16::from(e[1]) << 8;
				let palette = base + (e >> 10 & 7) as u8 * 4;
				for y in 0..8 {
					let line = if e & 0x8000 != 0 { 7 - y } else { y };
					let planes = bytes(tiles, usize::from(e & 0x3FF) * 16 + line * 2, 2)?;
					for x in 0..8 {
						let bit = if e & 0x4000 != 0 { x } else { 7 - x };
						let v = (planes[0] >> bit & 1) | (planes[1] >> bit & 1) << 1;
						if v != 0 {
							pixels[(r * 8 + y) * width + s * 256 + c * 8 + x] = palette + v;
						}
					}
				}
			}
		}
	}
	Ok(pixels)
}

/// An indexed PNG with colour 0 transparent.
pub fn write_png(
	path: &Path,
	width: u32,
	height: u32,
	palette: &[u8],
	pixels: &[u8],
) -> anyhow::Result<()> {
	let mut encoder = png::Encoder::new(BufWriter::new(fs::File::create(path)?), width, height);
	encoder.set_color(png::ColorType::Indexed);
	encoder.set_depth(png::BitDepth::Eight);
	encoder.set_palette(palette);
	encoder.set_trns(&[0][..]);
	encoder.write_header()?.write_image_data(pixels)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn colours_widen_bgr555() {
		assert_eq!(colours(&[0x1F, 0x7C]), [255, 0, 255]);
		assert_eq!(colours(&[0xE0, 0x03]), [0, 255, 0]);
		assert_eq!(colours(&[0x08, 0x00]), [66, 0, 0]);
	}

	#[test]
	fn tileset_reads_palettes_and_low_nibbles_first() {
		let mut tiles = vec![0; 256 + 32];
		tiles[0] = 0x30;
		tiles[256] = 0x21;
		let mut common = vec![0; 256 + 32];
		common[0] = 0x10;
		common[256 + 31] = 0x40;
		let pixels = tileset(&tiles, &common);
		assert_eq!(pixels[..3], [0x31, 0x32, 0x30]);
		// Tile 1 is missing from the stream.
		assert_eq!(pixels[8], 0);
		// Tile 192 is the first shared one, at row 12, column 0; its last pixel is (7, 7).
		assert_eq!(pixels[(12 * 8 + 7) * 128 + 7], 0x14);
	}

	#[test]
	fn sky_flips_tiles_and_skips_colour_0() {
		// Tile 1: a single colour-3 pixel at the top left.
		let mut tiles = vec![0; 32];
		tiles[16] = 0x80;
		tiles[17] = 0x80;
		let mut layout = vec![0; 1536];
		// Row 0, column 0: tile 1, palette 2, flipped both ways.
		layout[..2].copy_from_slice(&0xC801_u16.to_le_bytes());
		let pixels = sky(&tiles, &layout, 0, 4, 64).unwrap();
		assert_eq!(pixels[7 * 1024 + 7], 64 + 8 + 3);
		assert_eq!(pixels.iter().filter(|&&p| p != 0).count(), 1);
	}

	#[test]
	fn smk_rom_textures() {
		let Ok(path) = std::env::var("SMK_ROM") else {
			return;
		};
		let rom = Rom::parse(std::fs::read(path).unwrap()).unwrap();
		let folder = std::env::temp_dir().join(format!("desmack-textures-{}", std::process::id()));
		tilesets(&rom, &folder).unwrap();
		skies(&rom, &folder).unwrap();
		let textures = folder.join("textures");
		let size = |path: &str| {
			let reader = png::Decoder::new(fs::File::open(textures.join(path)).unwrap())
				.read_info()
				.unwrap();
			(reader.info().width, reader.info().height)
		};
		assert_eq!(size("tilesets/bowser-castle.png"), (128, 128));
		assert_eq!(size("skies/mario-circuit-near.png"), (1024, 32));
		assert_eq!(size("skies/mario-circuit-far.png"), (512, 32));
		fs::remove_dir_all(&folder).unwrap();
	}
}
