extern crate daemonize;
extern crate env_logger;
extern crate log;
extern crate clap;

mod clipboard;
mod recv;
mod source_data;

use daemonize::Daemonize;
use std::io::stdin;
use std::fs::File;
use clap::{Command, arg};

fn cli() -> Command {
    Command::new("richclip")
        .about("A fictional versioning CLI")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("copy")
            .about("Receive and copy data to the clipboard")
        )
}

fn main() {
    env_logger::init();

    let matches = cli().get_matches();
    match matches.subcommand() {
        Some(("copy", _sub_matches)) => {
            do_copy();
        }
        _ => unreachable!(),
    }
}

fn do_copy() {
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

    clipboard::copy_wayland(source_data);
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
