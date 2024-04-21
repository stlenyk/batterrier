// src: https://www.linuxuprising.com/2021/02/how-to-limit-battery-charging-set.html

mod linux_service;

use anyhow::{Error, Ok, Result};
use clap::{Parser, Subcommand};

use std::{fs, process};

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
    /// Change battery charge limit. Needs `sudo`
    Set {
        /// Battery charge % limit [0, 100]
        value: Percent,
    },
    /// Print current battery charge limit
    Get,
}

struct BatteryLimiter {
    bat_name: &'static str,
}
impl BatteryLimiter {
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

    fn get_value(&self) -> Result<Percent> {
        fs::read_to_string(format!(
            "/sys/class/power_supply/{}/charge_control_end_threshold",
            self.bat_name
        ))?
        .trim()
        .parse::<Percent>()
        .map_err(Error::msg)
    }

    fn set(&self, limit: Percent) -> Result<()> {
        let old_limit = self.get_value()?;
        if old_limit == limit {
            println!("🔋{} -> 🔋{}", old_limit, limit);
            return Ok(());
        }

        const SERVICE_FILENAME: &str = "battery-charge-threshold.service";
        let mut linux_service: LinuxService =
            serde_ini::from_str(include_str!("../battery-charge-threshold.service")).unwrap();

        linux_service.service.exec_start = format!(
            "/bin/bash -c 'echo {} > /sys/class/power_supply/{}/charge_control_end_threshold'",
            limit, self.bat_name
        );
        let service_contents = serde_ini::to_string(&linux_service)?;
        sudo::escalate_if_needed()
            .map_err(|e| e.to_string())
            .map_err(Error::msg)?;
        fs::write(
            const_format::formatcp!("/etc/systemd/system/{}", SERVICE_FILENAME),
            service_contents,
        )?;

        const COMMANDS: [&str; 3] = [
            const_format::formatcp!("systemctl enable --now {}", SERVICE_FILENAME),
            "systemctl daemon-reload",
            const_format::formatcp!("systemctl restart {}", SERVICE_FILENAME),
        ];
        for cmd in COMMANDS {
            let args = cmd.split(' ');
            process::Command::new("sudo").args(args).spawn()?.wait()?;
        }

        println!("🔋{} -> 🔋{}", old_limit, limit);

        Ok(())
    }

    fn get(&self) -> Result<()> {
        let charge_limit = self.get_value()?;
        println!("🔋{}", charge_limit);
        Ok(())
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let battery_limiter = BatteryLimiter::new()?;
    match args.command {
        Command::Set { value } => {
            battery_limiter.set(value)?;
        }
        Command::Get => {
            battery_limiter.get()?;
        }
    }

    Ok(())
}
