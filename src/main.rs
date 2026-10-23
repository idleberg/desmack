use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::{CommandFactory, Parser};
use clap_complete::{Shell, generate};

mod decompress;
mod maps;
mod rom;
use rom::Rom;

#[derive(Parser)]
#[command(
	name = "desmack",
	version,
	about = "Extracts the assets of a Super Mario Kart SNES ROM"
)]
struct Cli {
	#[arg(required_unless_present = "completions", help = "ROMs to extract")]
	roms: Vec<PathBuf>,

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

/// What to do with one ROM: write to `Ok(folder)`, or skip it with `Err(reason)`.
type Job = (PathBuf, Result<PathBuf, String>);

/// Resolves every ROM to its real path, drops repeats, and refuses any two ROMs that would share
/// an output folder.
fn plan(roms: &[PathBuf]) -> Vec<Job> {
	let mut seen = HashSet::new();
	let mut jobs: Vec<Job> = Vec::new();
	for rom in roms {
		match fs::canonicalize(rom) {
			Ok(real) if !seen.insert(real.clone()) => {}
			Ok(real) => {
				let folder = real.with_extension("");
				jobs.push((real, Ok(folder)));
			}
			Err(error) => jobs.push((rom.clone(), Err(error.to_string()))),
		}
	}

	let mut users: HashMap<PathBuf, usize> = HashMap::new();
	for (_, folder) in &jobs {
		if let Ok(folder) = folder {
			*users.entry(folder.clone()).or_default() += 1;
		}
	}
	for (_, folder) in &mut jobs {
		if let Ok(path) = folder
			&& users[path] > 1
		{
			*folder = Err(format!(
				"skipped, another ROM also writes to {}",
				path.display()
			));
		}
	}
	jobs
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

	let mut dialog =
		clark::group_multiselect("What should desmack extract?").initial_values(Category::ALL);
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

fn extract(
	rom: &Path,
	folder: &Path,
	categories: &[Category],
	force: bool,
	tty: bool,
) -> anyhow::Result<String> {
	let rom = Rom::parse(fs::read(rom).context("can't read it")?)?;
	if !may_write(folder, force, tty)? {
		bail!("skipped, {} exists", folder.display());
	}
	fs::create_dir_all(folder)
		.with_context(|| format!("skipped, can't write to {}", folder.display()))?;

	let (mut done, mut failed) = (Vec::new(), Vec::new());
	for &category in categories {
		let result = match category {
			Category::Maps => maps::extract(&rom, folder),
			_ => continue,
		};
		match result {
			Ok(()) => done.push(category.to_string()),
			Err(error) => failed.push(format!("{category} ({error:#})")),
		}
	}
	let mut line = if done.is_empty() {
		format!("{rom}, nothing extracted")
	} else {
		format!("{rom}, extracted {}", done.join(", "))
	};
	if !failed.is_empty() {
		line += &format!(", failed {}", failed.join(", "));
		bail!(line);
	}
	Ok(line)
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

	let tty = io::stdin().is_terminal();
	let categories = match choose(&cli, tty) {
		Ok(Some(categories)) => categories,
		Ok(None) => {
			clark::cancel("nothing extracted");
			return ExitCode::SUCCESS;
		}
		Err(error) => {
			eprintln!("{error:#}");
			return ExitCode::FAILURE;
		}
	};

	let mut ok = true;
	for (rom, folder) in plan(&cli.roms) {
		let line = folder
			.map_err(anyhow::Error::msg)
			.and_then(|folder| extract(&rom, &folder, &categories, cli.force, tty));
		match line {
			Ok(line) => eprintln!("{}: {line}", rom.display()),
			Err(error) => {
				eprintln!("{}: {error:#}", rom.display());
				ok = false;
			}
		}
	}
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

	#[test]
	fn plan_drops_repeats_and_refuses_shared_folders() {
		let dir = std::env::temp_dir().join(format!("desmack-plan-{}", std::process::id()));
		fs::create_dir_all(&dir).unwrap();
		for name in ["smk.sfc", "smk.smc", "other.sfc"] {
			fs::write(dir.join(name), b"").unwrap();
		}
		let roms = [
			"smk.sfc",
			"other.sfc",
			"smk.smc",
			"other.sfc",
			"missing.sfc",
		]
		.map(|name| dir.join(name));

		let jobs = plan(&roms);
		fs::remove_dir_all(&dir).unwrap();

		let outcome: Vec<_> = jobs
			.iter()
			.map(|(rom, folder)| (rom.file_name().unwrap().to_str().unwrap(), folder.is_ok()))
			.collect();
		assert_eq!(
			outcome,
			[
				("smk.sfc", false),
				("other.sfc", true),
				("smk.smc", false),
				("missing.sfc", false),
			]
		);
	}
}
