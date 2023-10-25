// src: https://www.linuxuprising.com/2021/02/how-to-limit-battery-charging-set.html

use anyhow::Ok;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::{fs, process};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LinuxService {
    unit: Unit,
    service: Service,
    install: Install,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Unit {
    description: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Service {
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    working_directory: Option<String>,
    exec_start: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    restart: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    restart_sec: Option<u32>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Install {
    wanted_by: String,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// needs `sudo`
    Set {
        /// battery charge % limit [0, 100]
        value: u8,
    },
    Get,
}
struct BatteryLimiter {}

impl BatteryLimiter {
    fn set(limit: u8) -> anyhow::Result<()> {
        if limit > 100 {
            anyhow::bail!("Limit must be <=100")
        }
        let mut linux_service: LinuxService =
            serde_ini::from_str(include_str!("../battery-charge-threshold.service")).unwrap();

        // TODO BAT0 is hardcoded
        linux_service.service.exec_start = format!(
            "/bin/bash -c 'echo {} > /sys/class/power_supply/BAT0/charge_control_end_threshold'",
            limit
        );
        let service_path = "/etc/systemd/system/battery-charge-threshold.service";
        let service_contents = serde_ini::to_string(&linux_service).unwrap();
        sudo::escalate_if_needed().unwrap();
        fs::write(service_path, service_contents)?;

        let commands = [
            "systemctl enable --now battery-charge-threshold.service",
            "systemctl daemon-reload",
            "systemctl restart battery-charge-threshold.service",
        ];
        for cmd in commands {
            let args = cmd.split(' ');
            process::Command::new("sudo")
                .args(args)
                .spawn()
                .unwrap()
                .wait()
                .unwrap();
        }

        Ok(())
    }

    fn get() -> anyhow::Result<()> {
        let charge_limit =
            fs::read_to_string("/sys/class/power_supply/BAT0/charge_control_end_threshold")
                .unwrap();
        println!("🔋{}", charge_limit.trim());
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Section {
    info: String,
}

#[derive(Serialize, Deserialize)]
struct Something {
    section: Section,
    another_section: Option<String>,
}

fn main() {
    let args = Args::parse();
    match args.command {
        Command::Set { value } => BatteryLimiter::set(value).unwrap(),
        Command::Get => BatteryLimiter::get().unwrap(),
    }
}
