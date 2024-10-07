# Batterrier

A CLI tool for setting battery charge limit of ASUS laptops on Linux.

[credits](https://www.linuxuprising.com/2021/02/how-to-limit-battery-charging-set.html)

## Installation

1. [Install Rust](https://www.rust-lang.org/tools/install)
2. Install the binary:

    ```sh
    cargo install --git=https://github.com/stlenyk/batterrier.git
    ```

## Usage

```sh
batterrier help
```

```
Usage: batterrier <COMMAND>

Commands:
  set    Change battery charge limit
  get    Print current battery charge limit
  clean  Restore 100% battery limit and remove systemd service
  info   Print battery info
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

---

```sh
batterrier help set
```

```
Usage: batterrier set [OPTIONS] <VALUE>

Arguments:
  <VALUE>  Battery charge % limit [0, 100]

Options:
  -p, --persist  Persist after system reboot, i.e. create a systemd service
  -h, --help     Print help
```

### Examples

```
$ batterrier set 60 --persist
🔋100 -> 🔋60
Creating systemd service
$ batterrier get
🔋60
```
