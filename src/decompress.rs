use anyhow::{Context, bail};

/// Unpacks one compressed stream, stopping at its `0xFF` end byte.
#[cfg_attr(
	not(test),
	expect(dead_code, reason = "the extractors call it from milestone 4")
)]
pub fn decompress(src: &[u8]) -> anyhow::Result<Vec<u8>> {
	let mut input = src.iter().copied();
	let mut next = || input.next().context("compressed stream ends early");
	let mut out: Vec<u8> = Vec::new();
	loop {
		let v = next()?;
		if v == 0xFF {
			return Ok(out);
		}
		let (command, count) = if v >> 5 == 7 {
			(
				(v >> 2) & 7,
				(usize::from(v & 3) << 8 | usize::from(next()?)) + 1,
			)
		} else {
			(v >> 5, usize::from(v & 0x1F) + 1)
		};
		match command {
			0 => {
				for _ in 0..count {
					out.push(next()?);
				}
			}
			1 => {
				let b = next()?;
				out.extend(std::iter::repeat_n(b, count));
			}
			2 => {
				let pair = [next()?, next()?];
				out.extend((0..count).map(|i| pair[i % 2]));
			}
			3 => {
				let b = next()?;
				out.extend((0..count).map(|i| b.wrapping_add(i as u8)));
			}
			4..=6 => {
				let start = match command {
					6 => out.len().checked_sub(usize::from(next()?)),
					_ => Some(usize::from(u16::from_le_bytes([next()?, next()?]))),
				};
				let xor = if command == 5 { 0xFF } else { 0 };
				for i in 0..count {
					let b = start
						.and_then(|start| out.get(start + i))
						.context("compressed stream copies from beyond its output")?;
					out.push(b ^ xor);
				}
			}
			_ => bail!("unknown compression command {command}"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::rom::{Region, Rom};

	#[test]
	fn decompress_runs_every_command() {
		let src = [
			0x02, 1, 2, 3, // literal
			0x22, 9, // repeat
			0x43, 4, 5, // alternate
			0x62, 0xFE, // increment
			0x81, 0, 0, // absolute copy
			0xA1, 1, 0, // inverted absolute copy
			0xC2, 3, // relative copy
			0xFF,
		];
		assert_eq!(
			decompress(&src).unwrap(),
			[
				1, 2, 3, 9, 9, 9, 4, 5, 4, 5, 0xFE, 0xFF, 0, 1, 2, 0xFD, 0xFC, 2, 0xFD, 0xFC
			]
		);
	}

	#[test]
	fn decompress_reads_long_headers() {
		// Command 1, count 0x100 + 1.
		let out = decompress(&[0xE5, 0x00, 7, 0xFF]).unwrap();
		assert_eq!(out, vec![7; 0x101]);
	}

	#[test]
	fn decompress_refuses_broken_streams() {
		assert!(decompress(&[0x01, 1]).is_err());
		assert!(decompress(&[0x80, 0, 0, 0xFF]).is_err());
		assert!(decompress(&[0xC0, 1, 0xFF]).is_err());
		assert!(decompress(&[0xFC, 0, 0xFF]).is_err());
	}

	#[test]
	fn smk_rom_track_0_decompresses_twice() {
		let Ok(path) = std::env::var("SMK_ROM") else {
			return;
		};
		let rom = Rom::parse(std::fs::read(path).unwrap()).unwrap();
		let pointer = |at: usize| {
			let b = &rom.data[at..at + 3];
			usize::from(b[2] & 0x0F) << 16 | usize::from(b[1]) << 8 | usize::from(b[0])
		};
		let list = pointer(match rom.region {
			Region::Japan => 0x1E74D,
			Region::Usa => 0x1E749,
			Region::Europe => 0x1E738,
		});
		let track = pointer(list);
		let packed = &rom.data[track..track + 3372];
		assert!(decompress(&packed[..3371]).is_err());
		let once = decompress(packed).unwrap();
		assert_eq!(once.len(), 5871);
		assert_eq!(decompress(&once).unwrap().len(), 16384);
	}
}
