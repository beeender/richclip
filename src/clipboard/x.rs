use super::mime_type::decide_mime_type;
use super::PasteConfig;
use crate::source_data::SourceData;
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

struct XClient {
    conn: RustConnection,
    win_id: u32,
    atoms: AtomCollection,
}

struct XPasteState<'a, T: AsFd + Write> {
    supported_mime_types: Option<Vec<String>>,
    config: PasteConfig<'a, T>,
    // Translate the config.primary
    selection: Atom,
}

struct XCopyState<'a> {
    finishied: bool,
    source_data: &'a dyn SourceData,
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

fn targets_to_strings(client: &mut XClient, reply: &GetPropertyReply) -> Result<Vec<String>> {
    if reply.type_ != client.atoms.ATOM {
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
        match get_atom_name(&client.conn, v) {
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

fn create_x_client(display_name: Option<&str>) -> Result<XClient> {
    let (conn, screen_num) =
        x11rb::connect(display_name).context("Failed to connect to the X server")?;
    let screen = &conn.setup().roots[screen_num];
    let win_id = conn.generate_id()?;

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
    )
    .context("Failed to call 'create_window'")?;

    let atoms = AtomCollection::new(&conn)?;
    let atoms = atoms.reply()?;
    Ok(XClient {
        conn,
        atoms,
        win_id,
    })
}

pub fn paste_x<T: AsFd + Write + 'static>(config: PasteConfig<T>) -> Result<()> {
    let mut client = create_x_client(None)?;

    let selection = if config.use_primary {
        client.atoms.PRIMARY
    } else {
        client.atoms.CLIPBOARD
    };
    // Use 'TARGETS' to list all supported mime-types of the clipboard content first
    client
        .conn
        .convert_selection(
            client.win_id,
            selection,
            client.atoms.TARGETS,
            client.atoms.XCLIP_OUT,
            CURRENT_TIME,
        )
        .context("Failed to call convert_selection to get 'TARGETS'")?;
    client.conn.flush().context("Failed to flush connection")?;

    let mut state = XPasteState {
        supported_mime_types: None,
        config,
        selection,
    };

    loop {
        let event = client
            .conn
            .wait_for_event()
            .context("Failed to get X event")?;
        match event {
            Event::SelectionNotify(event) => {
                if event.selection != state.selection {
                    continue;
                }
                let reply = client
                    .conn
                    .get_property(
                        false,
                        client.win_id,
                        client.atoms.XCLIP_OUT,
                        AtomEnum::ANY,
                        0,
                        u32::MAX,
                    )
                    .context("get_property 'XCLIP_OUT' failed")?
                    .reply()
                    .context("'XCLIP_OUT' reply failed")?;
                if state.supported_mime_types.is_none() {
                    if reply.type_ == AtomEnum::NONE.into() {
                        log::debug!(
                            "Got None type reply which probably means the clipboard is empty"
                        );
                        break;
                    }
                    let mime_types = targets_to_strings(&mut client, &reply)
                        .context("Failed to get supported targets")?;
                    if mime_types.is_empty() {
                        log::debug!("Got 0 targets which probably means the clipboard is empty");
                        break;
                    }
                    if state.config.list_types_only {
                        for line in mime_types {
                            writeln!(state.config.fd_to_write, "{}", line)
                                .context("Failed to write to the output")?;
                        }
                        break;
                    } else {
                        let mime_type =
                            decide_mime_type(&state.config.expected_mime_type, &mime_types)
                                .unwrap_or("".to_string());
                        if mime_type.is_empty() {
                            log::debug!("Failed to decide mime type");
                            break;
                        }
                        let target = get_atom_id_by_name(&client.conn, &mime_type)
                            .context(format!("Failed to get atom id for '{}'", mime_type))?;
                        state.supported_mime_types = Some(mime_types);
                        client
                            .conn
                            .convert_selection(
                                client.win_id,
                                selection,
                                target,
                                client.atoms.XCLIP_OUT,
                                CURRENT_TIME,
                            )
                            .context("Failed to call convert_selection to get 'TARGETS'")?;
                        client.conn.flush().context("Failed to flush connection")?;
                    }
                } else if reply.type_ == client.atoms.INCR {
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
