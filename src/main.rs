extern crate clap;
extern crate daemonize;
extern crate env_logger;
extern crate log;

mod clipboard;
mod recv;
mod source_data;

use clap::{Arg, ArgMatches, Command};
use daemonize::Daemonize;
use std::fs::File;
use std::io::{stdin, stdout};

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
}

fn main() {
    env_logger::init();
    let matches = cli().get_matches();
    match matches.subcommand() {
        Some(("copy", sub_matches)) => {
            do_copy(sub_matches);
        }
        Some(("paste", sub_matches)) => {
            do_paste(sub_matches);
        }
        _ => unreachable!(),
    }
}

fn do_copy(arg_matches: &ArgMatches) {
    let stdin = stdin();
    let source_data = recv::receive_data(&stdin).unwrap();

    // Move to background. We fork our process and leave the child running in the background, while
    // exiting in the parent. We also replace stdin/stdout with /dev/null so the stdout file
    // descriptor isn't kept alive, and chdir to the root, to prevent blocking file systems from
    // being unmounted.
    // The above is copied from wl-clipboard.
    let in_null = File::create("/dev/null").unwrap();
    let out_null = File::create("/dev/null").unwrap();
    let daemonize = Daemonize::new()
        .working_directory("/") // prevent blocking fs from being unmounted.
        .stdout(in_null)
        .stderr(out_null);

    // wl-clipboard does this
    ignore_sighub();
    match daemonize.start() {
        Ok(_) => println!("Success, daemonized"),
        Err(e) => eprintln!("Error, {}", e),
    }

    let copy_config = clipboard::CopyConfig {
        source_data,
        use_primary: *arg_matches.get_one::<bool>("primary").unwrap(),
    };
    match choose_backend() {
        Backend::Wayland => {
            clipboard::copy_wayland(copy_config).expect("Failed to copy to wayland clipboard")
        }
        Backend::X => clipboard::copy_x(copy_config).expect("Failed to copy to wayland clipboard"),
    }
}

fn do_paste(arg_matches: &ArgMatches) {
    let t = match arg_matches.get_one::<String>("type") {
        Some(t) => t,
        _ => "",
    };
    let cfg = clipboard::PasteConfig {
        list_types_only: *arg_matches.get_one::<bool>("list-types").unwrap(),
        use_primary: *arg_matches.get_one::<bool>("primary").unwrap(),
        fd_to_write: &mut stdout(),
        expected_mime_type: t.to_string(),
    };
    match choose_backend() {
        Backend::Wayland => {
            clipboard::paste_wayland(cfg).expect("Failed to paste from wayland clipboard")
        }
        Backend::X => clipboard::paste_x(cfg).expect("Failed to paste from X clipboard"),
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
