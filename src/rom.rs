use std::fmt;

use anyhow::{bail, ensure};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Region {
	Japan,
	Usa,
	Europe,
}

impl Region {
	fn flag(self) -> &'static str {
		match self {
			Region::Japan => "🇯🇵",
			Region::Usa => "🇺🇸",
			Region::Europe => "🇪🇺",
		}
	}
}

/// The known dumps, by CRC32 without a copier header. A beta's header can name another Region, or
/// be missing, so the Region here is the one the dump is credited to.
const DUMPS: [(u32, Region, &str); 7] = [
	(0xC800_2453, Region::Japan, "v1.0"),
	(0xCD80_DB86, Region::Usa, "v1.0"),
	(0x5641_0E5E, Region::Europe, "v1.0"),
	(0x6AAF_031A, Region::Japan, "Beta 1"),
	(0x8DC4_E42F, Region::Japan, "Beta 2"),
	(0xF575_F2A0, Region::Japan, "Beta 3"),
	(0x54F5_E6D4, Region::Usa, "Beta"),
];

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
	pub crc: u32,
	/// The known dump's revision, or `None` for a modified ROM.
	pub revision: Option<&'static str>,
}

impl Rom {
	pub fn parse(mut data: Vec<u8>) -> anyhow::Result<Rom> {
		if data.len() % 0x40000 == 512 {
			data.drain(..512);
		}
		let crc = crc32fast::hash(&data);
		if let Some(&(_, region, revision)) = DUMPS.iter().find(|dump| dump.0 == crc) {
			return Ok(Rom {
				data,
				region,
				crc,
				revision: Some(revision),
			});
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
		Ok(Rom {
			data,
			region,
			crc,
			revision: None,
		})
	}

	/// Betas keep their assets elsewhere, so they are named but not extracted.
	pub fn beta(&self) -> bool {
		self.revision
			.is_some_and(|revision| revision.starts_with("Beta"))
	}
}

impl fmt::Display for Rom {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(
			f,
			"{} Super Mario Kart {:08X} ({})",
			self.region.flag(),
			self.crc,
			self.revision.unwrap_or("unknown")
		)
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
		assert_eq!(rom.region, Region::Europe);
		assert_eq!(rom.revision, None);
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
		assert!(rom.revision.is_some_and(|_| !rom.beta()), "{rom}");
	}
}
