use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context, ensure};

use crate::decompress::decompress;
use crate::rom::{Region, Rom};

/// In course byte order.
const COURSES: [&str; 24] = [
	"mario-circuit-3",
	"ghost-valley-2",
	"donut-plains-2",
	"bowser-castle-2",
	"vanilla-lake-2",
	"rainbow-road",
	"koopa-beach-2",
	"mario-circuit-1",
	"ghost-valley-3",
	"bowser-castle-3",
	"choco-island-2",
	"donut-plains-3",
	"vanilla-lake-1",
	"koopa-beach-1",
	"mario-circuit-4",
	"mario-circuit-2",
	"ghost-valley-1",
	"bowser-castle-1",
	"choco-island-1",
	"donut-plains-1",
	"battle-course-3",
	"battle-course-4",
	"battle-course-1",
	"battle-course-2",
];

/// In theme number order.
pub const THEMES: [&str; 8] = [
	"ghost-valley",
	"mario-circuit",
	"donut-plains",
	"choco-island",
	"vanilla-lake",
	"koopa-beach",
	"bowser-castle",
	"rainbow-road",
];

const GENRES: usize = 0x7FDBA;
const HAZARDS: usize = 0x5C800;
const OVERLAYS: usize = 0x5D000;
/// Ghost Valley's pillar lists, one per course: GV1, GV2, GV3.
const PILLARS: usize = 0x4DD15;

/// Where each Region keeps its map tables.
pub struct Offsets {
	pub track_list: usize,
	overlay_patterns: usize,
	overlay_sizes: usize,
	ai_bank: usize,
	ai_zones: usize,
	starts: usize,
}

impl Offsets {
	pub fn of(region: Region) -> Offsets {
		match region {
			Region::Japan => Offsets {
				track_list: 0x1E74D,
				overlay_patterns: 0x4F0B5,
				overlay_sizes: 0x4F1FC,
				ai_bank: 0x1FBC4,
				ai_zones: 0x1FF8C,
				starts: 0x18A8D,
			},
			Region::Usa => Offsets {
				track_list: 0x1E749,
				overlay_patterns: 0x4F23D,
				overlay_sizes: 0x4F384,
				ai_bank: 0x1FBD3,
				ai_zones: 0x1FF9B,
				starts: 0x18A79,
			},
			Region::Europe => Offsets {
				track_list: 0x1E738,
				overlay_patterns: 0x4F159,
				overlay_sizes: 0x4F2A0,
				ai_bank: 0x1FB9D,
				ai_zones: 0x1FF6D,
				starts: 0x18A92,
			},
		}
	}
}

/// ROM reads that fail instead of panicking when a modified ROM points past its end.
pub fn bytes(data: &[u8], at: usize, len: usize) -> anyhow::Result<&[u8]> {
	data.get(at..at + len)
		.with_context(|| format!("ROM ends before {at:#X}"))
}

fn byte(data: &[u8], at: usize) -> anyhow::Result<u8> {
	Ok(bytes(data, at, 1)?[0])
}

pub fn word(data: &[u8], at: usize) -> anyhow::Result<usize> {
	let b = bytes(data, at, 2)?;
	Ok(usize::from(b[0]) | usize::from(b[1]) << 8)
}

/// A word that addresses the same bank as `at`.
pub fn near(data: &[u8], at: usize) -> anyhow::Result<usize> {
	Ok(at & 0xFF_0000 | word(data, at)?)
}

pub fn long(data: &[u8], at: usize) -> anyhow::Result<usize> {
	Ok(usize::from(byte(data, at + 2)? & 0x3F) << 16 | word(data, at)?)
}

/// Decompresses the stream that starts at `at`.
pub fn stream(data: &[u8], at: usize) -> anyhow::Result<Vec<u8>> {
	decompress(
		data.get(at..)
			.with_context(|| format!("ROM ends before {at:#X}"))?,
	)
}

/// x and y in tiles, packed as the hazard and overlay tables pack them.
fn tile(b: &[u8]) -> (usize, usize) {
	(
		usize::from(b[0] & 0x7F),
		usize::from(b[1] & 0x3F) << 1 | usize::from(b[0] >> 7),
	)
}

pub fn extract(rom: &Rom, folder: &Path) -> anyhow::Result<()> {
	let data = &rom.data;
	let o = Offsets::of(rom.region);
	let list = long(data, o.track_list)?;

	let genres = folder.join("maps/genres");
	fs::create_dir_all(&genres)?;
	let stream = stream(data, GENRES)?;
	for (theme, name) in THEMES.into_iter().enumerate() {
		// The game reads 192 bytes from the theme's offset, which sits 16 bytes before the track
		// list, then the stream's first 64. Koopa Beach and Rainbow Road run past the stream's
		// end into whatever RAM holds; none of their courses use those tiles.
		let start = word(data, list - 16 + theme * 2)?;
		let mut text = String::new();
		for index in 0..256 {
			let at = if index < 192 {
				start + index
			} else {
				index - 192
			};
			let genre = stream.get(at).map_or("undefined", |&g| genre(g));
			writeln!(text, "{index:02x} {genre}")?;
		}
		fs::write(genres.join(format!("{name}.txt")), text)?;
	}

	for (course, name) in COURSES.into_iter().enumerate() {
		let dir = folder.join("maps").join(name);
		fs::create_dir_all(&dir)?;
		let (gp, tt) = tile_maps(data, &o, list, course)?;
		fs::write(dir.join("gp.map"), map_text(&gp))?;
		fs::write(dir.join("tt.map"), map_text(&tt))?;
		fs::write(dir.join("hazards.txt"), hazards(data, course)?)?;
		fs::write(dir.join("ai.txt"), ai(data, &o, course)?)?;
		fs::write(dir.join("start.txt"), starts(data, &o, course)?)?;
	}
	Ok(())
}

/// The GP and TT tile maps: the TT map is compressed twice, and the GP map adds the Overlay.
fn tile_maps(
	data: &[u8],
	o: &Offsets,
	list: usize,
	course: usize,
) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
	let packed = long(data, list + course * 3)?;
	let tt = decompress(&stream(data, packed)?)?;
	ensure!(
		tt.len() == 0x4000,
		"{} is not 128×128 tiles",
		COURSES[course]
	);

	let mut gp = tt.clone();
	for entry in bytes(data, OVERLAYS + course * 128, 128)?
		.as_chunks::<3>()
		.0
	{
		if *entry == [0xFF; 3] {
			break;
		}
		let pattern = near(data, o.overlay_patterns + usize::from(entry[0] & 0x3F) * 2)?;
		let size = bytes(data, o.overlay_sizes + usize::from(entry[0] >> 6) * 2, 2)?;
		let (width, height) = (usize::from(size[0]), usize::from(size[1]));
		let at = (usize::from(entry[1]) | usize::from(entry[2]) << 8) & 0x3FFF;
		for (i, &t) in bytes(data, pattern, width * height)?.iter().enumerate() {
			let cell = at + i / width * 128 + i % width;
			if t != 0xFF && cell < gp.len() {
				gp[cell] = t;
			}
		}
	}
	Ok((gp, tt))
}

fn map_text(map: &[u8]) -> String {
	map.chunks(128)
		.map(|row| {
			let row: Vec<_> = row.iter().map(|t| format!("{t:02x}")).collect();
			row.join(" ") + "\n"
		})
		.collect()
}

/// Ghost Valley's pillars come from their own lists, the battle courses have no hazards, and
/// every other course has 16 regular and 6 Match Race entries, with 0 marking an empty one.
fn hazards(data: &[u8], course: usize) -> anyhow::Result<String> {
	let mut text = String::new();
	if let Some(n) = [16, 1, 8].iter().position(|&c| c == course) {
		let mut at = near(data, PILLARS + n * 2)? + 2;
		while word(data, at)? != 0 {
			let b = bytes(data, at, 2)?;
			writeln!(text, "{} {}", b[0], b[1])?;
			at += 2;
		}
	} else if course < 20 {
		for (i, b) in bytes(data, HAZARDS + course * 64, 44)?
			.chunks(2)
			.enumerate()
		{
			if b != [0, 0] {
				let (x, y) = tile(b);
				let race = if i >= 16 { " match-race" } else { "" };
				writeln!(text, "{x} {y}{race}")?;
			}
		}
	}
	Ok(text)
}

/// One line per AI zone, in tiles: a rectangle's corner and size, or a triangle's right-angle
/// corner and leg length, then the zone's AI target.
fn ai(data: &[u8], o: &Offsets, course: usize) -> anyhow::Result<String> {
	let bank = usize::from(byte(data, o.ai_bank)? & 0x3F) << 16;
	let mut zone = bank | word(data, o.ai_zones + course * 2)?;
	let mut target = bank | word(data, o.ai_zones + 0x30 + course * 2)?;
	let mut text = String::new();
	loop {
		let shape = match byte(data, zone)? {
			0xFF => return Ok(text),
			0 => "rectangle",
			2 => "triangle-top-left",
			4 => "triangle-top-right",
			6 => "triangle-bottom-right",
			8 => "triangle-bottom-left",
			other => anyhow::bail!("unknown AI zone shape {other}"),
		};
		let len = if shape == "rectangle" { 4 } else { 3 };
		let size: Vec<_> = bytes(data, zone + 1, len)?
			.iter()
			.map(|&v| (usize::from(v) * 2).to_string())
			.collect();
		let t = bytes(data, target, 3)?;
		let cross = if t[2] & 0x80 != 0 {
			" intersection"
		} else {
			""
		};
		let (x, y, speed) = (t[0], t[1], t[2] & 3);
		writeln!(text, "{shape} {} {x} {y} {speed}{cross}", size.join(" "))?;
		zone += 1 + len;
		target += 3;
	}
}

/// In pixels. GP courses place 8 karts in two columns, 24 pixels apart; battle courses list the
/// 2 players.
fn starts(data: &[u8], o: &Offsets, course: usize) -> anyhow::Result<String> {
	let record = near(data, near(data, o.starts + course * 2)?)? + 2;
	let mut text = String::new();
	if course < 20 {
		let (x, y) = (word(data, record)?, word(data, record + 2)?);
		let step = word(data, record + 4)? as i16 as isize;
		for kart in 0..8 {
			let x = x as isize + step * (kart % 2);
			writeln!(text, "{x} {}", y + 24 * kart as usize)?;
		}
	} else {
		for player in 0..2 {
			let at = record + player * 4;
			writeln!(text, "{} {}", word(data, at)?, word(data, at + 2)?)?;
		}
	}
	Ok(text)
}

/// Epic Edit's genre names.
fn genre(g: u8) -> &'static str {
	match g {
		0x10 => "jump-bar",
		0x12 => "bump-edge",
		0x14 => "item-block",
		0x16 => "zipper",
		0x18 => "oil-slick",
		0x1A => "coin",
		0x1C => "bump",
		0x1E => "wood-plank",
		0x20 => "empty",
		0x22 => "deep-water",
		0x24 => "lava",
		0x26 => "out-of-bounds",
		0x28 => "empty-border",
		0x40 => "road",
		0x42 => "plank-junction",
		0x44 => "brick-road",
		0x46 => "clay",
		0x48 => "wet-sand",
		0x4A => "soft-sand",
		0x4C => "rough-clay",
		0x4E => "flat-surface",
		0x50 => "bridge",
		0x52 => "slippery-road",
		0x54 => "sand",
		0x56 => "off-road",
		0x58 => "snow",
		0x5A => "grass",
		0x5C => "shallow-water",
		0x5E => "mud",
		0x80 => "solid-block",
		0x82 => "frail-block",
		0x84 => "ice-block",
		_ => "unknown",
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn smk_rom_maps() {
		let Ok(path) = std::env::var("SMK_ROM") else {
			return;
		};
		let rom = Rom::parse(fs::read(path).unwrap()).unwrap();
		let dir = std::env::temp_dir().join(format!("desmack-maps-{}", std::process::id()));
		extract(&rom, &dir).unwrap();
		let read = |file: &str| fs::read_to_string(dir.join("maps").join(file)).unwrap();
		let cell = |file: &str, x: usize, y: usize| {
			read(file)
				.lines()
				.nth(y)
				.unwrap()
				.split(' ')
				.nth(x)
				.unwrap()
				.to_owned()
		};

		// A ? panel on Mario Circuit 1, in GP only.
		assert_eq!(cell("mario-circuit-1/gp.map", 6, 32), "c4");
		assert_ne!(cell("mario-circuit-1/tt.map", 6, 32), "c4");
		assert!(read("genres/mario-circuit.txt").starts_with("00 out-of-bounds\n"));
		assert!(read("genres/koopa-beach.txt").contains("\n8c undefined\n"));
		assert!(read("mario-circuit-2/hazards.txt").starts_with("85 29\n"));
		assert!(read("ghost-valley-1/hazards.txt").starts_with("115 21\n"));
		assert!(read("mario-circuit-1/ai.txt").starts_with("rectangle 102 58 26 12 112 53 1\n"));
		// The grid recorded from a GP race in snes9x.
		assert!(read("mario-circuit-1/start.txt").starts_with("920 588\n952 612\n920 636\n"));
		assert_eq!(read("battle-course-4/start.txt"), "512 392\n512 632\n");
		fs::remove_dir_all(&dir).unwrap();
	}
}
