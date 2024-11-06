// src: https://www.linuxuprising.com/2021/02/how-to-limit-battery-charging-set.html

mod linux_service;

use anyhow::{Context, Error, Ok, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use regex::Regex;

use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    process::{self, Stdio},
};

use linux_service::LinuxService;

#[derive(Clone, Debug, PartialEq)]
struct Percent(u8);
impl std::str::FromStr for Percent {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const ERR_MSG: &str = "Percent must be a number between 0 and 100";
        let value = s.parse().map_err(|_e| ERR_MSG)?;
        if value > 100 {
            return std::result::Result::Err(ERR_MSG.to_owned());
        }
        std::result::Result::Ok(Self(value))
    }
}
impl std::fmt::Display for Percent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Change battery charge limit
    Set {
        #[arg(short, long, default_value_t = false)]
        /// Persist after system reboot, i.e. create a systemd service
        persist: bool,
        /// Battery charge % limit [0, 100]
        value: Percent,
    },
    /// Print current battery charge limit
    Get,
    /// Restore 100% battery limit and remove systemd service
    Clean,
    /// Print battery info
    Info,
    /// Generate shell completions
    #[command(long_about = "Generate shell completions
        Example:
        $ batterrier completions zsh > _batterrier
        $ sudo mv _batterrier /usr/local/share/zsh/site-functions")]
    Completions { shell: Shell },
}

struct BatteryLimiter {
    bat_path: PathBuf,
}
impl BatteryLimiter {
    const SERVICE_FILENAME: &'static str = "battery-charge-threshold.service";
    const SERVICE_PATH: &'static str =
        const_format::formatcp!("/etc/systemd/system/{}", BatteryLimiter::SERVICE_FILENAME);

    fn new() -> Result<Self> {
        // Path to the battery charge limit file is `/sys/class/power_supply/BAT?/charge_control_end_threshold`
        // where  `BAT?` is one of `BAT0`, `BAT1`, `BATT`, `BATC`.
        const BAT_NAME: [&str; 4] = ["BAT0", "BAT1", "BATT", "BATC"];
        for bat_name in &BAT_NAME {
            let bat_path = Path::new("/sys/class/power_supply").join(bat_name);
            if fs::metadata(&bat_path).is_ok() {
                return Ok(Self { bat_path });
            }
        }
        Err(Error::msg("Battery not found".to_owned()))
    }

    /// Write to a file with sudo. Equivalent to:
    /// ```sh
    /// echo $2 | sudo tee $1 > /dev/null
    /// ```
    fn write_protected<P: AsRef<Path>, C: AsRef<OsStr>>(path: P, contents: C) -> Result<()> {
        let echo = process::Command::new("echo")
            .arg(contents)
            .stdout(Stdio::piped())
            .spawn()?;
        process::Command::new("sudo")
            .arg("tee")
            .arg(path.as_ref().as_os_str())
            .stdin(Stdio::from(
                echo.stdout
                    .ok_or("No piped input from echo")
                    .map_err(Error::msg)?,
            ))
            .stdout(Stdio::null())
            .spawn()?
            .wait()?;
        Ok(())
    }

    fn print_changed_limit(old_limit: &Percent, new_limit: &Percent) {
        println!("🔋{old_limit} -> 🔋{new_limit}");
    }

    fn charge_control_threshold_path(&self) -> PathBuf {
        self.bat_path.join("charge_control_end_threshold")
    }

    fn get_value(&self) -> Result<Percent> {
        fs::read_to_string(self.charge_control_threshold_path())
            .context(format!(
                "Failed to read from {}",
                self.charge_control_threshold_path().display()
            ))?
            .trim()
            .parse::<Percent>()
            .map_err(|e| Error::msg(format!("Failed to parse battery limit: {e}")))
    }

    fn set_value(&self, limit: &Percent) -> Result<()> {
        Self::write_protected(self.charge_control_threshold_path(), limit.to_string())
    }

    fn set(&self, limit: &Percent, persist: bool) -> Result<()> {
        let old_limit = self.get_value()?;
        self.set_value(limit)?;
        Self::print_changed_limit(&old_limit, limit);

        if !persist {
            return Ok(());
        }

        println!("Creating systemd service");

        let mut linux_service: LinuxService =
            serde_ini::from_str(include_str!("../battery-charge-threshold.service")).unwrap();

        linux_service.service.exec_start = format!(
            "/bin/bash -c 'echo {} > {}'",
            limit,
            self.charge_control_threshold_path().display()
        );
        let service_contents = serde_ini::to_string(&linux_service)?;

        Self::write_protected(BatteryLimiter::SERVICE_PATH, service_contents)?;

        process::Command::new("sudo")
            .args(
                const_format::formatcp!("systemctl enable {}", BatteryLimiter::SERVICE_FILENAME)
                    .split(' '),
            )
            .spawn()?
            .wait()?;

        Ok(())
    }

    fn get_persisted(&self) -> Option<Percent> {
        let persisted_service: LinuxService =
            serde_ini::from_str(&fs::read_to_string(Self::SERVICE_PATH).ok()?).ok()?;
        let re = Regex::new(r"/bin/bash -c 'echo \b(\d+)\b > /sys/class/power_supply/BAT0/charge_control_end_threshold'").unwrap();
        re.captures(&persisted_service.service.exec_start)?
            .get(1)?
            .as_str()
            .parse()
            .ok()
    }

    fn get(&self) -> Result<()> {
        let current_limit = self.get_value()?;
        let persisted_limit = self.get_persisted();
        println!("current: 🔋{current_limit}");
        println!(
            "persisted: {}",
            if let Some(persisted_limit) = persisted_limit {
                format!("🔋{persisted_limit}")
            } else {
                "Not set".to_owned()
            }
        );

        Ok(())
    }

    fn clean(&self) -> Result<()> {
        let old_limit = self.get_value()?;
        self.set_value(&Percent(100))?;
        Self::print_changed_limit(&old_limit, &Percent(100));

        if fs::metadata(BatteryLimiter::SERVICE_PATH).is_ok() {
            println!("Removing systemd service");
            process::Command::new("sudo")
                .arg("rm")
                .arg(BatteryLimiter::SERVICE_PATH)
                .spawn()?
                .wait()?;
        }

        Ok(())
    }

    fn info(&self) {
        const INFO_FILES: [&str; 18] = [
            "alarm",
            "capacity",
            "capacity_level",
            "charge_control_end_threshold",
            "cycle_count",
            "energy_full",
            "energy_full_design",
            "energy_now",
            "manufacturer",
            "model_name",
            "power_now",
            "present",
            "serial_number",
            "status",
            "technology",
            "type",
            "voltage_min_design",
            "voltage_now",
        ];

        let info = INFO_FILES
            .iter()
            .filter_map(|file| {
                fs::read_to_string(self.bat_path.join(file))
                    .ok()
                    .map(|value| (file, value.trim().to_owned()))
            })
            .collect::<Vec<_>>();
        let pad_size = info.iter().map(|(file, _)| file.len()).max().unwrap_or(0);
        let info_string = info
            .iter()
            .map(|(file, value)| format!("{file:<pad_size$} {value}"))
            .collect::<Vec<_>>()
            .join("\n");
        let info_string = format!("Path: {}\n{info_string}", self.bat_path.display());

        println!("{info_string}");
    }
}

fn main() -> Result<()> {
    let args = Cli::parse();
    let battery_limiter = BatteryLimiter::new()?;

    match args.command {
        Command::Set { persist, value } => {
            battery_limiter.set(&value, persist)?;
        }
        Command::Get => {
            battery_limiter.get()?;
        }
        Command::Clean => {
            battery_limiter.clean()?;
        }
        Command::Info => battery_limiter.info(),
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                env!("CARGO_PKG_NAME"),
                &mut io::stdout(),
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::linux_service::LinuxService;

    #[test]
    fn service() {
        let service: Result<LinuxService, _> =
            serde_ini::from_str(include_str!("../battery-charge-threshold.service"));
        assert!(service.is_ok());
    }
}
