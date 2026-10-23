use std::fmt;

use anyhow::{bail, ensure};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Region {
	Japan,
	Usa,
	Europe,
}

impl Region {
	/// The CRC32 of the known dump, without a copier header.
	fn crc(self) -> u32 {
		match self {
			Region::Japan => 0xC800_2453,
			Region::Usa => 0xCD80_DB86,
			Region::Europe => 0x5641_0E5E,
		}
	}
}

impl fmt::Display for Region {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(match self {
			Region::Japan => "Japan",
			Region::Usa => "USA",
			Region::Europe => "Europe",
		})
	}
}

pub struct Rom {
	/// Without the copier header, so file offsets match the ROM facts.
	pub data: Vec<u8>,
	pub region: Region,
	pub revision: u8,
	pub known: bool,
}

impl Rom {
	pub fn parse(mut data: Vec<u8>) -> anyhow::Result<Rom> {
		if data.len() % 0x40000 == 512 {
			data.drain(..512);
		}
		ensure!(
			data.len() >= 0x10000
				&& &data[0xFFC0..0xFFD0] == b"SUPER MARIO KART"
				&& data[0xFFD5] == 0x31
				&& data[0xFFD6] == 0x05
				&& data[0xFFD8] == 0x01,
			"not a Super Mario Kart ROM"
		);
		let region = match data[0xFFD9] {
			0 => Region::Japan,
			1 => Region::Usa,
			2 => Region::Europe,
			other => bail!("unsupported Region {other}"),
		};
		let known = crc32fast::hash(&data) == region.crc();
		Ok(Rom {
			revision: data[0xFFDB],
			region,
			known,
			data,
		})
	}
}

impl fmt::Display for Rom {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let kind = if self.known { "known" } else { "modified" };
		write!(f, "{} 1.{}, {kind}", self.region, self.revision)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn header(region: u8) -> Vec<u8> {
		let mut data = vec![0; 0x40000];
		data[0xFFC0..0xFFD0].copy_from_slice(b"SUPER MARIO KART");
		data[0xFFD5] = 0x31;
		data[0xFFD6] = 0x05;
		data[0xFFD8] = 0x01;
		data[0xFFD9] = region;
		data
	}

	#[test]
	fn parse_strips_the_copier_header_and_reads_the_region() {
		let mut data = vec![0xAA; 512];
		data.extend(header(2));
		let rom = Rom::parse(data).unwrap();
		assert_eq!(rom.data.len(), 0x40000);
		assert_eq!(rom.to_string(), "Europe 1.0, modified");
	}

	#[test]
	fn parse_refuses_other_games_and_regions() {
		let mut other = header(1);
		other[0xFFC0] = b'X';
		assert!(Rom::parse(other).is_err());
		assert!(Rom::parse(header(3)).is_err());
		assert!(Rom::parse(vec![0; 100]).is_err());
	}

	#[test]
	fn smk_rom_is_known() {
		let Ok(path) = std::env::var("SMK_ROM") else {
			return;
		};
		let rom = Rom::parse(std::fs::read(path).unwrap()).unwrap();
		assert!(rom.known, "{rom}");
	}
}
