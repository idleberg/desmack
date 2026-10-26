use std::fs;
use std::path::Path;

use anyhow::{Context, ensure};

use crate::rom::Rom;

/// The block list the game uploads to the APU at boot, at the same offset in every Region.
const UPLOAD: usize = 0x28000;
/// The sample directory in ARAM (DSP `DIR = $3C`): a start and a loop address per sample.
const DIR: usize = 0x3C00;
const DIR_ENTRIES: usize = 28;
/// Pitch `$1000` plays a sample at this rate.
const RATE: u32 = 32000;

pub fn samples(rom: &Rom, folder: &Path) -> anyhow::Result<()> {
	let dir = folder.join("audio/samples");
	fs::create_dir_all(&dir)?;
	let ram = aram(rom)?;
	for (index, entry) in ram[DIR..DIR + DIR_ENTRIES * 4].chunks(4).enumerate() {
		let start = usize::from(u16::from_le_bytes([entry[0], entry[1]]));
		let restart = usize::from(u16::from_le_bytes([entry[2], entry[3]]));
		// Unused entries.
		if start == 0xFFFF {
			continue;
		}
		let (samples, restart) = brr(&ram, start, restart)?;
		fs::write(dir.join(format!("{index:02}.wav")), wav(&samples, restart))?;
	}
	Ok(())
}

/// ARAM after the boot upload: blocks of `u16 length, u16 address, data`, ended by a block of
/// length 0. The uploader only reads the upper half of each bank.
fn aram(rom: &Rom) -> anyhow::Result<Vec<u8>> {
	let stream: Vec<u8> = (UPLOAD..rom.data.len())
		.filter(|p| p & 0x8000 != 0)
		.map(|p| rom.data[p])
		.collect();
	let mut ram = vec![0; 0x10000];
	let mut at = 0;
	loop {
		let header = stream.get(at..at + 4).context("upload runs past the ROM")?;
		let length = usize::from(u16::from_le_bytes([header[0], header[1]]));
		let address = usize::from(u16::from_le_bytes([header[2], header[3]]));
		if length == 0 {
			return Ok(ram);
		}
		let data = stream
			.get(at + 4..at + 4 + length)
			.context("upload runs past the ROM")?;
		ram.get_mut(address..address + length)
			.context("upload runs past ARAM")?
			.copy_from_slice(data);
		at += 4 + length;
	}
}

/// BRR: 9-byte blocks of a header (shift, filter, loop and end flags) and 16 signed nibbles, high
/// nibble first, decoded by fullsnes' formulas. Returns the samples and, if the sample loops, the
/// sample its loop starts at.
fn brr(ram: &[u8], start: usize, restart: usize) -> anyhow::Result<(Vec<i16>, Option<usize>)> {
	let (mut old, mut older) = (0i32, 0i32);
	let mut samples = Vec::new();
	let mut at = start;
	loop {
		let block = ram.get(at..at + 9).context("sample runs past ARAM")?;
		let (shift, filter) = (block[0] >> 4, block[0] >> 2 & 3);
		for &byte in &block[1..] {
			for nibble in [byte >> 4, byte & 15] {
				let n = i32::from((nibble << 4) as i8 >> 4);
				let s = match shift {
					0..=12 => n << shift >> 1,
					_ if n < 0 => -2048,
					_ => 0,
				};
				let s = match filter {
					0 => s,
					1 => s + old + (-old >> 4),
					2 => s + 2 * old + ((-3 * old) >> 5) - older + (older >> 4),
					_ => s + 2 * old + ((-13 * old) >> 6) - older + ((3 * older) >> 4),
				};
				// Clamped to 16 bits, then wrapped to 15.
				let s = i32::from((s.clamp(-0x8000, 0x7FFF) as i16).wrapping_mul(2) >> 1);
				(older, old) = (old, s);
				samples.push((s * 2) as i16);
			}
		}
		if block[0] & 1 != 0 {
			if block[0] & 2 == 0 {
				return Ok((samples, None));
			}
			ensure!(
				restart >= start && restart <= at && (restart - start).is_multiple_of(9),
				"loop at {restart:04X} is not a block of the sample at {start:04X}"
			);
			return Ok((samples, Some((restart - start) / 9 * 16)));
		}
		at += 9;
	}
}

/// Mono 16-bit PCM at 32 kHz, with the loop, if any, in a `smpl` chunk.
fn wav(samples: &[i16], restart: Option<usize>) -> Vec<u8> {
	let words = |words: &[u32]| -> Vec<u8> { words.iter().flat_map(|w| w.to_le_bytes()).collect() };
	// Format 1 (PCM) and 1 channel, then the byte rate, then block align 2 and 16 bits.
	let mut chunks = chunk(b"fmt ", &words(&[0x0001_0001, RATE, RATE * 2, 0x0010_0002]));
	chunks.extend(chunk(
		b"data",
		&samples
			.iter()
			.flat_map(|s| s.to_le_bytes())
			.collect::<Vec<_>>(),
	));
	if let Some(restart) = restart {
		// Sample period in ns and unity note 60, then one forward loop, to the last sample.
		let end = samples.len() as u32 - 1;
		chunks.extend(chunk(
			b"smpl",
			&words(&[
				0,
				0,
				1_000_000_000 / RATE,
				60,
				0,
				0,
				0,
				1,
				0,
				0,
				0,
				restart as u32,
				end,
				0,
				0,
			]),
		));
	}
	let mut file = b"RIFF".to_vec();
	file.extend((chunks.len() as u32 + 4).to_le_bytes());
	file.extend(b"WAVE");
	file.extend(chunks);
	file
}

fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
	let mut chunk = id.to_vec();
	chunk.extend((body.len() as u32).to_le_bytes());
	chunk.extend(body);
	chunk
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn brr_shifts_filters_and_loops() {
		let mut ram = vec![0; 18];
		// Shift 12, filter 0: nibble 1 is 2048, nibble -1 is -2048, doubled on output.
		ram[0] = 0xC0;
		ram[1] = 0x1F;
		ram[8] = 0x0F;
		// Filter 1, shift 0, end and loop: zeros carry on from the last sample, -2048.
		ram[9] = 0x07;
		let (samples, restart) = brr(&ram, 0, 9).unwrap();
		assert_eq!(samples.len(), 32);
		assert_eq!(&samples[..2], [4096, -4096]);
		// -2048 + 0 + (2048 >> 4) = -1920.
		assert_eq!(samples[16], -3840);
		assert_eq!(restart, Some(16));

		ram[9] = 0x01;
		assert_eq!(brr(&ram, 0, 9).unwrap().1, None);
		assert!(brr(&ram[..9], 0, 0).is_err());
	}

	#[test]
	fn wav_has_a_loop_only_when_asked() {
		let plain = wav(&[1, -1], None);
		assert_eq!(&plain[..4], b"RIFF");
		assert_eq!(plain.len(), 12 + 24 + 12);
		assert_eq!(u32::from_le_bytes(plain[4..8].try_into().unwrap()), 40);
		assert_eq!(&plain[44..], [1, 0, 0xFF, 0xFF]);
		let looped = wav(&[1, -1], Some(1));
		assert_eq!(looped.len(), plain.len() + 8 + 60);
		assert_eq!(&looped[48..52], b"smpl");
	}

	#[test]
	fn rom_samples() {
		let Ok(path) = std::env::var("SMK_ROM") else {
			return;
		};
		let rom = Rom::parse(fs::read(path).unwrap()).unwrap();
		let folder = std::env::temp_dir().join(format!("desmack-audio-{}", std::process::id()));
		samples(&rom, &folder).unwrap();
		let dir = folder.join("audio/samples");
		assert_eq!(fs::read_dir(&dir).unwrap().count(), 26);
		// 2640 samples, looping from the fifth block.
		let first = fs::read(dir.join("00.wav")).unwrap();
		assert_eq!(
			u32::from_le_bytes(first[40..44].try_into().unwrap()),
			2640 * 2
		);
		assert_eq!(&first[44 + 2640 * 2..][..4], b"smpl");
		fs::remove_dir_all(folder).unwrap();
	}
}
