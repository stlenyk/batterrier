// src: https://www.linuxuprising.com/2021/02/how-to-limit-battery-charging-set.html

mod linux_service;

use anyhow::{Error, Ok, Result};
use clap::{Parser, Subcommand};

use std::{
    fs,
    process::{self, Stdio},
};

use linux_service::LinuxService;

#[derive(Clone, PartialEq)]
struct Percent(u8);
impl std::str::FromStr for Percent {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const ERR_MSG: &str = "Percent must be an number between 0 and 100";
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
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Change battery charge limit
    Set {
        #[arg(short, long, default_value = "false")]
        /// Persist after system reboot, i.e. create a systemd service
        persist: bool,
        /// Battery charge % limit [0, 100]
        value: Percent,
    },
    /// Print current battery charge limit
    Get,
    /// Restore 100% battery limit and remove systemd service
    Clean,
}

struct BatteryLimiter {
    bat_name: &'static str,
}
impl BatteryLimiter {
    const SERVICE_FILENAME: &'static str = "battery-charge-threshold.service";
    const SERVICE_PATH: &'static str =
        const_format::formatcp!("/etc/systemd/system/{}", BatteryLimiter::SERVICE_FILENAME);

    fn new() -> Result<Self> {
        const BAT_NAME: [&str; 4] = ["BAT0", "BAT1", "BATT", "BATC"];
        for bat_name in BAT_NAME.iter() {
            let path = format!("/sys/class/power_supply/{}", bat_name);
            if fs::metadata(&path).is_ok() {
                return Ok(Self { bat_name });
            }
        }
        Err(Error::msg("Battery not found".to_owned()))
    }

    fn write_protected(path: &str, contents: &str) -> Result<()> {
        let echo = process::Command::new("echo")
            .arg(contents)
            .stdout(Stdio::piped())
            .spawn()?;
        process::Command::new("sudo")
            .arg("tee")
            .arg(path)
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
        println!("🔋{} -> 🔋{}", old_limit, new_limit);
    }

    fn get_value(&self) -> Result<Percent> {
        fs::read_to_string(format!(
            "/sys/class/power_supply/{}/charge_control_end_threshold",
            self.bat_name
        ))?
        .trim()
        .parse::<Percent>()
        .map_err(|e| Error::msg(format!("Failed to parse battery limit: {}", e)))
    }

    fn set_value(&self, limit: &Percent) -> Result<()> {
        Self::write_protected(
            &format!(
                "/sys/class/power_supply/{}/charge_control_end_threshold",
                self.bat_name
            ),
            &limit.to_string(),
        )
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
            "/bin/bash -c 'echo {} > /sys/class/power_supply/{}/charge_control_end_threshold'",
            limit, self.bat_name
        );
        let service_contents = serde_ini::to_string(&linux_service)?;

        Self::write_protected(BatteryLimiter::SERVICE_PATH, &service_contents)?;

        process::Command::new("sudo")
            .args(
                const_format::formatcp!("systemctl enable {}", BatteryLimiter::SERVICE_FILENAME)
                    .split(' '),
            )
            .spawn()?
            .wait()?;

        Ok(())
    }

    fn get(&self) -> Result<()> {
        let charge_limit = self.get_value()?;
        println!("🔋{}", charge_limit);
        Ok(())
    }

    fn clean(&self) -> Result<()> {
        let old_limit = self.get_value()?;
        self.set_value(&Percent(100))?;
        Self::print_changed_limit(&old_limit, &Percent(100));

        if fs::metadata(BatteryLimiter::SERVICE_PATH).is_ok() {
            process::Command::new("sudo")
                .arg("rm")
                .arg(BatteryLimiter::SERVICE_PATH)
                .spawn()?
                .wait()?;
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
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
    }

    Ok(())
}
