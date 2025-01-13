extern crate clap;
extern crate daemonize;
extern crate log;
extern crate simplelog;

mod clipboard;
mod protocol;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
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

/// Clipboard utility for multiple platforms
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Arguments for copy command
#[derive(Args)]
struct CopyArgs {
    /// Use the 'primary' clipboard
    #[arg(long = "primary", short = 'p', num_args = 0)]
    primary: bool,
    /// Run in foreground
    #[arg(long = "foreground", num_args = 0)]
    foreground: bool,
    /// For testing X INCR mode
    #[arg(
        long = "chunk-size",
        hide = true,
        required = false,
        num_args = 1,
        default_value = "0"
    )]
    chunk_size: usize,
}

/// Arguments for paste command
#[derive(Args)]
struct PasteArgs {
    /// List the offered mime-types of the current clipboard only without the contents
    #[arg(long = "list-types", short = 'l', num_args = 0)]
    list_types: bool,
    /// Specify the preferred mime-type to be pasted
    #[arg(
        long = "type",
        short = 't',
        value_name = "mime-type",
        num_args = 1,
        default_value = ""
    )]
    type_: String,
    /// Use the 'primary' clipboard
    #[arg(long = "primary", short = 'p', num_args = 0)]
    primary: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Receive and copy data to the clipboard
    Copy(CopyArgs),
    /// Paste the data from clipboard to the output
    Paste(PasteArgs),
    /// Print version info
    Version,
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

    let cli = Cli::parse();

    match cli.command {
        Commands::Copy(copy_args) => do_copy(&copy_args)?,
        Commands::Paste(paste_args) => do_paste(&paste_args)?,
        Commands::Version => {
            let ver = env!("CARGO_PKG_VERSION");
            let git_desc = env!("VERGEN_GIT_DESCRIBE");
            let build_date = env!("VERGEN_BUILD_DATE");
            let target = env!("VERGEN_CARGO_TARGET_TRIPLE");
            println!("richclip {ver} ({git_desc} {target} {build_date})");
        }
    }

    Ok(())
}

fn do_copy(copy_args: &CopyArgs) -> Result<()> {
    let stdin = stdin();
    let source_data = protocol::receive_data(&stdin).context("Failed to read data from stdin")?;

    // Move to background. We fork our process and leave the child running in the background, while
    // exiting in the parent. We also replace stdin/stdout with /dev/null so the stdout file
    // descriptor isn't kept alive, and chdir to the root, to prevent blocking file systems from
    // being unmounted.
    // The above is copied from wl-clipboard.
    let out_null = File::create("/dev/null")?;
    let err_null = File::create("/dev/null")?;

    if !copy_args.foreground {
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
        use_primary: copy_args.primary,
        x_chunk_size: copy_args.chunk_size,
    };
    match choose_backend() {
        Backend::Wayland => {
            clipboard::copy_wayland(copy_config).context("Failed to copy to wayland clipboard")
        }
        Backend::X => clipboard::copy_x(copy_config).context("Failed to copy to wayland clipboard"),
    }
}

fn do_paste(paste_args: &PasteArgs) -> Result<()> {
    let cfg = clipboard::PasteConfig {
        list_types_only: paste_args.list_types,
        use_primary: paste_args.primary,
        writter: &mut stdout(),
        expected_mime_type: paste_args.type_.clone(),
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
