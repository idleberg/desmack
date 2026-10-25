use std::fmt;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use clap::{CommandFactory, Parser};
use clap_complete::{Shell, generate};

mod decompress;
mod graphics;
mod maps;
mod rom;
mod sprites;
mod textures;
use rom::Rom;

#[derive(Parser)]
#[command(
	name = "desmack",
	version,
	about = "Extracts the assets of a Super Mario Kart SNES ROM"
)]
struct Cli {
	#[arg(required_unless_present = "completions", help = "ROM to extract")]
	rom: Option<PathBuf>,

	#[arg(long, help = "Extract driver sprites")]
	drivers: bool,
	#[arg(long, help = "Extract item sprites")]
	items: bool,
	#[arg(long, help = "Extract hazard sprites")]
	hazards: bool,
	#[arg(long, help = "Extract cast sprites")]
	cast: bool,
	#[arg(long, help = "Extract effect sprites")]
	effects: bool,
	#[arg(long, help = "Extract the HUD")]
	hud: bool,
	#[arg(long, help = "Extract the font")]
	font: bool,
	#[arg(long, help = "Extract the screens")]
	screens: bool,
	#[arg(long, help = "Extract the tilesets")]
	tilesets: bool,
	#[arg(long, help = "Extract the skies")]
	skies: bool,
	#[arg(long, help = "Extract the course maps")]
	maps: bool,

	#[arg(long, help = "Extract every category, without asking")]
	all: bool,

	#[arg(long, help = "Overwrite output folders without asking")]
	force: bool,

	#[arg(
		long,
		value_name = "SHELL",
		exclusive = true,
		help = "Print a shell completion script"
	)]
	completions: Option<Shell>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Category {
	Drivers,
	Items,
	Hazards,
	Cast,
	Effects,
	Hud,
	Font,
	Screens,
	Tilesets,
	Skies,
	Maps,
}

impl Category {
	/// In flag order, which is also dialog order.
	const ALL: [Category; 11] = [
		Category::Drivers,
		Category::Items,
		Category::Hazards,
		Category::Cast,
		Category::Effects,
		Category::Hud,
		Category::Font,
		Category::Screens,
		Category::Tilesets,
		Category::Skies,
		Category::Maps,
	];

	/// The dialog group, which is also the top-level output folder.
	fn group(self) -> &'static str {
		match self {
			Category::Drivers
			| Category::Items
			| Category::Hazards
			| Category::Cast
			| Category::Effects => "sprites",
			Category::Hud | Category::Font | Category::Screens => "graphics",
			Category::Tilesets | Category::Skies => "textures",
			Category::Maps => "maps",
		}
	}
}

impl fmt::Display for Category {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(&format!("{self:?}").to_lowercase())
	}
}

impl Cli {
	fn flagged(&self) -> Vec<Category> {
		let flags = [
			self.drivers,
			self.items,
			self.hazards,
			self.cast,
			self.effects,
			self.hud,
			self.font,
			self.screens,
			self.tilesets,
			self.skies,
			self.maps,
		];
		Category::ALL
			.into_iter()
			.zip(flags)
			.filter_map(|(category, flag)| flag.then_some(category))
			.collect()
	}
}

/// The categories the user asked for: flags first, then the dialog, then everything.
/// `None` means the dialog was cancelled.
fn choose(cli: &Cli, tty: bool) -> anyhow::Result<Option<Vec<Category>>> {
	let flagged = cli.flagged();
	if cli.all || (!tty && flagged.is_empty()) {
		return Ok(Some(Category::ALL.to_vec()));
	}
	if !flagged.is_empty() {
		return Ok(Some(flagged));
	}

	let mut dialog = clark::group_multiselect("What should desmack extract?");
	for group in ["sprites", "graphics", "textures", "maps"] {
		dialog = dialog.group(
			group,
			Category::ALL.into_iter().filter(|c| c.group() == group),
		);
	}
	Ok(dialog.interact_opt()?)
}

/// Whether to write into `folder`, by the overwrite rules.
fn may_write(folder: &Path, force: bool, tty: bool) -> anyhow::Result<bool> {
	if force || !folder.is_dir() {
		return Ok(true);
	}
	if !tty {
		return Ok(false);
	}
	let answer = clark::confirm(format!("{} exists. Overwrite?", folder.display()))
		.initial_value(false)
		.interact_opt()?;
	Ok(answer == Some(true))
}

fn extract(category: Category, rom: &Rom, folder: &Path) -> anyhow::Result<()> {
	match category {
		Category::Drivers => sprites::drivers(rom, folder),
		Category::Items => sprites::items(rom, folder),
		Category::Hazards => sprites::hazards(rom, folder),
		Category::Cast => sprites::cast(rom, folder),
		Category::Effects => sprites::effects(rom, folder),
		Category::Hud => graphics::hud(rom, folder),
		Category::Font => graphics::font(rom, folder),
		Category::Screens => graphics::screens(rom, folder),
		Category::Tilesets => textures::tilesets(rom, folder),
		Category::Skies => textures::skies(rom, folder),
		Category::Maps => maps::extract(rom, folder),
	}
}

fn main() -> ExitCode {
	let cli = Cli::parse();

	// Buffered, then written in one go: `generate` unwraps its writes, so handing it stdout
	// directly turns `desmack --completions zsh | head` into a panic.
	if let Some(shell) = cli.completions {
		let mut script = Vec::new();
		generate(shell, &mut Cli::command(), "desmack", &mut script);
		return match io::stdout().write_all(&script) {
			Err(error) if error.kind() != io::ErrorKind::BrokenPipe => {
				eprintln!("{error}");
				ExitCode::FAILURE
			}
			_ => ExitCode::SUCCESS,
		};
	}

	// `required_unless_present` guarantees a ROM once `--completions` is handled.
	let path = cli.rom.as_deref().unwrap();
	let rom = match fs::read(path).context("can't read it").and_then(Rom::parse) {
		Ok(rom) => rom,
		Err(error) => {
			eprintln!("{}: {error:#}", path.display());
			return ExitCode::FAILURE;
		}
	};
	clark::intro(rom.to_string());
	if rom.beta() {
		clark::cancel("betas can't be extracted yet");
		return ExitCode::FAILURE;
	}

	let tty = io::stdin().is_terminal();
	let folder = path.with_extension("");
	let categories = match choose(&cli, tty) {
		Ok(Some(categories)) => categories,
		Ok(None) => {
			clark::cancel("nothing extracted");
			return ExitCode::SUCCESS;
		}
		Err(error) => {
			clark::cancel(format!("{error:#}"));
			return ExitCode::FAILURE;
		}
	};
	match may_write(&folder, cli.force, tty) {
		Ok(true) => {}
		Ok(false) => {
			clark::cancel(format!("skipped, {} exists", folder.display()));
			return ExitCode::FAILURE;
		}
		Err(error) => {
			clark::cancel(format!("{error:#}"));
			return ExitCode::FAILURE;
		}
	}
	if let Err(error) = fs::create_dir_all(&folder) {
		clark::cancel(format!("can't write to {}: {error}", folder.display()));
		return ExitCode::FAILURE;
	}

	// Off a terminal, one line per message rather than frames.
	let ci = !io::stdout().is_terminal();
	let mut ok = true;
	for category in categories {
		let spinner = clark::spinner()
			.ci(ci)
			.start(format!("extracting {category}"));
		match extract(category, &rom, &folder) {
			Ok(()) => spinner.stop(format!("extracted {category}")),
			Err(error) => {
				spinner.error(format!("{category} failed: {error:#}"));
				ok = false;
			}
		}
	}
	clark::outro(format!("saved to {}", folder.display()));
	if ok {
		ExitCode::SUCCESS
	} else {
		ExitCode::FAILURE
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn completions_name_the_binary_and_every_flag() {
		let mut script = Vec::new();
		generate(Shell::Bash, &mut Cli::command(), "desmack", &mut script);
		let script = String::from_utf8(script).unwrap();
		for category in Category::ALL {
			assert!(script.contains(&format!("--{category}")), "{category}");
		}
		assert!(script.contains("--completions"));
	}
}
