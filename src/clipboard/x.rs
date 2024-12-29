extern crate x11rb;

use super::mime_type::decide_mime_type;
use super::PasteConfig;
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::os::fd::AsFd;
use x11rb::atom_manager;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::{COPY_DEPTH_FROM_PARENT, CURRENT_TIME};

atom_manager! {
    pub AtomCollection: AtomCollectionCookie {
        // For the selection type
        PRIMARY,
        CLIPBOARD,
        // For slection content mime-type, AKA the target
        TARGETS,
        // Our defined atom for getting prop
        XCLIP_OUT,
        // Others
        INCR,
        ATOM,
    }
}

struct XPasteState<'a, T: AsFd + Write> {
    conn: RustConnection,
    atoms: AtomCollection,

    supported_mime_types: Option<Vec<String>>,
    config: PasteConfig<'a, T>,
    // Translate the config.primary
    selection: Atom,
}

fn get_atom_id_by_name(conn: &RustConnection, name: &str) -> Result<Atom> {
    let result = conn.intern_atom(false, name.as_bytes()).context("")?;
    let id = result.reply().context("")?;
    Ok(id.atom)
}

fn get_atom_name(conn: &RustConnection, atom: Atom) -> Result<String> {
    let result = conn.get_atom_name(atom).context("")?.reply().context("")?;
    let str = String::from_utf8(result.name).context("")?;
    Ok(str)
}

fn targets_to_strings<T: AsFd + Write>(
    state: &mut XPasteState<T>,
    reply: &GetPropertyReply,
) -> Result<Vec<String>> {
    if reply.type_ != state.atoms.ATOM {
        bail!(
            "'TARGETS' selection returned an unexpected type {}",
            reply.type_
        );
    }

    let mut ret = Vec::<String>::new();
    let it = reply
        .value32()
        .context("'targets_to_strings' got reply with wrong value type")?;
    for v in it {
        match get_atom_name(&state.conn, v) {
            Ok(name) => {
                ret.push(name);
            }
            Err(error) => {
                log::error!("Failed to get name for Atom '{}', error: {}", v, error);
            }
        }
    }

    Ok(ret)
}

pub fn paste_x<T: AsFd + Write + 'static>(config: PasteConfig<T>) -> Result<()> {
    let (conn, screen_num) = x11rb::connect(None).context("Failed to connect to the X server")?;
    let screen = &conn.setup().roots[screen_num];
    let win_id = conn.generate_id()?;

    let atoms = AtomCollection::new(&conn)?;
    let atoms = atoms.reply()?;

    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        win_id,
        screen.root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new().background_pixel(screen.white_pixel),
    )?;

    let selection = if config.use_primary {
        atoms.PRIMARY
    } else {
        atoms.CLIPBOARD
    };
    // Use 'TARGETS' to list all supported mime-types of the clipboard content first
    conn.convert_selection(
        win_id,
        selection,
        atoms.TARGETS,
        atoms.XCLIP_OUT,
        CURRENT_TIME,
    )
    .context("Failed to call convert_selection to get 'TARGETS'")?;
    conn.flush().context("Failed to flush connection")?;

    let mut state = XPasteState {
        conn,
        atoms,
        supported_mime_types: None,
        config,
        selection,
    };

    loop {
        let event = state
            .conn
            .wait_for_event()
            .context("Failed to get X event")?;
        match event {
            Event::SelectionNotify(event) => {
                if event.selection != state.selection {
                    continue;
                }
                let reply = state
                    .conn
                    .get_property(false, win_id, atoms.XCLIP_OUT, AtomEnum::ANY, 0, u32::MAX)
                    .context("get_property 'XCLIP_OUT' failed")?
                    .reply()
                    .context("'XCLIP_OUT' reply failed")?;
                if state.supported_mime_types.is_none() {
                    let mime_types = targets_to_strings(&mut state, &reply)
                        .context("Failed to get supported targets")?;
                    if state.config.list_types_only {
                        for line in mime_types {
                            writeln!(state.config.fd_to_write, "{}", line)
                                .context("Failed to write to the output")?;
                        }
                        break;
                    } else {
                        let mime_type =
                            decide_mime_type(&state.config.expected_mime_type, &mime_types)
                                .context("Failed to decide mime type")?;
                        let target = get_atom_id_by_name(&state.conn, &mime_type)
                            .context(format!("Failed to get atom id for '{}'", mime_type))?;
                        state.supported_mime_types = Some(mime_types);
                        state
                            .conn
                            .convert_selection(
                                win_id,
                                selection,
                                target,
                                atoms.XCLIP_OUT,
                                CURRENT_TIME,
                            )
                            .context("Failed to call convert_selection to get 'TARGETS'")?;
                        state.conn.flush().context("Failed to flush connection")?;
                    }
                } else if reply.type_ == atoms.INCR {
                    bail!("INCR has not been implemented")
                } else {
                    state
                        .config
                        .fd_to_write
                        .write(&reply.value)
                        .context("Failed to write to the output")?;
                    break;
                }
            }
            Event::PropertyNotify(_) => {
                bail!("INCR has not been implemented")
            }
            _ => {}
        }
    }
    Ok(())
}
