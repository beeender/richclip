extern crate clap;
extern crate daemonize;
extern crate log;
extern crate simplelog;

mod clipboard;
mod protocol;

use anyhow::{Context, Result};
use clap::{value_parser, Arg, ArgMatches, Command};
use daemonize::Daemonize;
use std::env;
use std::fs::File;
use std::io::{stdin, stdout};
use std::str::FromStr;

enum Backend {
    Wayland,
    X,
}

fn choose_backend() -> Backend {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        return Backend::Wayland;
    } else if std::env::var("DISPLAY").is_ok() {
        return Backend::X;
    }

    log::error!(
        "Failed to decide which backend to use. '$WAYLAND_DISPLAY' or '$DISPLAY' env needs to be set"
    );
    std::process::exit(1)
}

fn cli() -> Command {
    Command::new("richclip")
        .about("A fictional versioning CLI")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("copy")
            .about("Receive and copy data to the clipboard")
            .arg(
                    Arg::new("primary")
                        .long("primary")
                        .short('p')
                        .required(false)
                        .num_args(0)
                        .help("Use the 'primary' clipboard")
                )
            .arg(
                    Arg::new("foreground")
                        .long("foreground")
                        .required(false)
                        .num_args(0)
                        .help("Run in foreground")
                )
            .arg(
                    Arg::new("chunk-size")
                        .long("chunk-size")
                        .value_parser(value_parser!(usize))
                        .default_value("0")
                        .required(false)
                        .hide(true)
                        .num_args(1)
                        .help("For testing X INCR mode")
                )
        )
        .subcommand(
            Command::new("paste")
                .about("Paste the data from clipboard to the output")
                .arg(
                    Arg::new("list-types")
                        .long("list-types")
                        .short('l')
                        .required(false)
                        .num_args(0)
                        .help("List the offered mime-types of the current clipboard only without the contents")
                )
                .arg(
                    Arg::new("type")
                        .long("type")
                        .short('t')
                        .value_name("mime-type")
                        .required(false)
                        .num_args(1)
                        .help("Specify the preferred mime-type to be pasted")
                )
                .arg(
                    Arg::new("primary")
                        .long("primary")
                        .short('p')
                        .required(false)
                        .num_args(0)
                        .help("Use the 'primary' clipboard")
                ),
        )
        .subcommand(
                Command::new("version")
                .about("Print version info")
        )
}

fn init_logger() -> Result<()> {
    use simplelog::{
        ColorChoice, CombinedLogger, ConfigBuilder, LevelFilter, SharedLogger, TermLogger,
        TerminalMode, WriteLogger,
    };

    let log_path = env::var("RICHCLIP_LOG_FILE").unwrap_or("".to_string());
    let level_str = env::var("RICHCLIP_LOG_LEVEL").unwrap_or("Warn".to_string());
    let level = LevelFilter::from_str(&level_str).unwrap_or(log::LevelFilter::Warn);
    let config = ConfigBuilder::default()
        .set_time_offset_to_local()
        .expect("Failed to set time offset to local for loggers")
        .build();
    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        level,
        config.clone(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )];
    if !log_path.is_empty() {
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .expect("Cannot open the log file at '$RICHCLIP_LOG_FILE'");
        loggers.push(WriteLogger::new(LevelFilter::Debug, config, log_file));
    }
    CombinedLogger::init(loggers).context("Failed to initialize loggers")?;
    Ok(())
}

fn main() -> Result<()> {
    init_logger()?;

    let matches = cli().get_matches();
    match matches.subcommand() {
        Some(("copy", sub_matches)) => {
            do_copy(sub_matches)?;
        }
        Some(("paste", sub_matches)) => {
            do_paste(sub_matches)?;
        }
        Some(("version", _)) => {
            let ver = env!("CARGO_PKG_VERSION");
            let git_desc = env!("VERGEN_GIT_DESCRIBE");
            let build_date = env!("VERGEN_BUILD_DATE");
            let target = env!("VERGEN_CARGO_TARGET_TRIPLE");
            println!("richclip {ver} ({git_desc} {target} {build_date})");
        }
        _ => unreachable!(),
    }

    Ok(())
}

fn do_copy(arg_matches: &ArgMatches) -> Result<()> {
    let stdin = stdin();
    let source_data = protocol::receive_data(&stdin).context("Failed to read data from stdin")?;
    let foreground = *arg_matches
        .get_one::<bool>("foreground")
        .context("`--foreground` option is not specified for the `copy` command")?;

    // Move to background. We fork our process and leave the child running in the background, while
    // exiting in the parent. We also replace stdin/stdout with /dev/null so the stdout file
    // descriptor isn't kept alive, and chdir to the root, to prevent blocking file systems from
    // being unmounted.
    // The above is copied from wl-clipboard.
    let out_null = File::create("/dev/null")?;
    let err_null = File::create("/dev/null")?;

    if !foreground {
        let daemonize = Daemonize::new()
            .working_directory("/") // prevent blocking fs from being unmounted.
            .stdout(out_null)
            .stderr(err_null);

        // wl-clipboard does this
        ignore_sighub();
        daemonize.start()?;
    }

    let copy_config = clipboard::CopyConfig {
        source_data,
        use_primary: *arg_matches
            .get_one::<bool>("primary")
            .context("`--primary` option is not specified for the `copy` command")?,
        x_chunk_size: *arg_matches
            .get_one::<usize>("chunk-size")
            .context("`--chunk-size` option is not specified for the `copy` command")?,
    };
    match choose_backend() {
        Backend::Wayland => {
            clipboard::copy_wayland(copy_config).context("Failed to copy to wayland clipboard")
        }
        Backend::X => clipboard::copy_x(copy_config).context("Failed to copy to wayland clipboard"),
    }
}

fn do_paste(arg_matches: &ArgMatches) -> Result<()> {
    let t = match arg_matches.get_one::<String>("type") {
        Some(t) => t,
        _ => "",
    };
    let cfg = clipboard::PasteConfig {
        list_types_only: *arg_matches
            .get_one::<bool>("list-types")
            .context("`--list-types` option is not specified for the `paste` command")?,
        use_primary: *arg_matches
            .get_one::<bool>("primary")
            .context("`--primary` option is not specified for the `paste` command")?,
        writter: &mut stdout(),
        expected_mime_type: t.to_string(),
    };
    match choose_backend() {
        Backend::Wayland => {
            clipboard::paste_wayland(cfg).context("Failed to paste from wayland clipboard")
        }
        Backend::X => clipboard::paste_x(cfg).context("Failed to paste from X clipboard"),
    }
}

fn ignore_sighub() {
    use core::ffi::c_int;
    use core::ffi::c_void;
    extern "C" {
        fn signal(sig: c_int, handler: *const c_void);
    }

    const SIGHUB: i32 = 1;
    const SIG_IGN: *const c_void = 1 as *const c_void;
    unsafe {
        signal(SIGHUB, SIG_IGN);
    }
}
