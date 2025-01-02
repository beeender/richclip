use super::mime_type::decide_mime_type;
use super::CopyConfig;
use super::PasteConfig;
use crate::protocol::SourceData;
use anyhow::{bail, Context, Result};
use std::collections::hash_map::HashMap;
use std::io::Write;
use std::os::fd::AsFd;
use std::rc::Rc;
use x11rb::atom_manager;
use x11rb::connection::Connection;
use x11rb::connection::RequestConnection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt, CreateWindowAux, EventMask,
    GetPropertyReply, PropMode, Property, SelectionNotifyEvent, SelectionRequestEvent, Window,
    WindowClass, SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;
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

#[derive(PartialEq)]
enum TransferResult {
    Done,
    Continue,
}

// To handle both normal selection sending and INCR mode sending.
struct XSelectionSender {
    requestor: Window,
    // Clipboard or Primary
    selection: Atom,
    // Target AKA mime-types.
    target: Atom,
    // Identifier created by the receiver
    property: Atom,
    // The content type, for 'TARGETS', it is 'ATOM'. Otherwise it will be the same as target
    content_type: Atom,
    // The reference to the actual data
    content: Rc<Vec<u8>>,
    // If the data need to be sent in INCR mode
    chunk_size: usize,
    // The current content offset for INCR mode. Initialized with MAX value.
    offset: usize,
}

struct XCopyState<'a> {
    source_data: &'a dyn SourceData,
    ongoing_senders: HashMap<Window, XSelectionSender>,
}

impl XSelectionSender {
    fn new(
        client: &XClient,
        event: &SelectionRequestEvent,
        content_type: Atom,
        content: Rc<Vec<u8>>,
    ) -> Self {
        XSelectionSender {
            requestor: event.requestor,
            selection: event.selection,
            target: event.target,
            property: event.property,
            content_type,
            content,
            chunk_size: Self::get_chunk_size(&client.conn),
            offset: usize::MAX,
        }
    }

    fn get_chunk_size(conn: &RustConnection) -> usize {
        // See xclip.c::xcin()
        conn.maximum_request_bytes() / 4
    }

    // The sending is actually calling X window change_property API, and the other side could use
    // get_property to retrieve the data.
    fn change_property_to_send(&mut self, conn: &RustConnection) -> Result<()> {
        log::debug!(
            "change_property_to_send total length {}, offset {}",
            self.content.len(),
            self.offset
        );
        let left_bytes = self.content.len() - self.offset;
        let end_pos = if self.chunk_size > left_bytes {
            self.offset + left_bytes
        } else {
            self.offset + self.chunk_size
        };
        let to_send = &self.content[self.offset..end_pos];

        conn.change_property8(
            PropMode::REPLACE,
            self.requestor,
            self.property,
            self.content_type,
            to_send,
        )?;
        self.offset = end_pos;
        Ok(())
    }

    fn send(&mut self, client: &XClient, time: u32) -> Result<TransferResult> {
        if self.chunk_size > self.content.len() {
            self.offset = 0;
            self.change_property_to_send(&client.conn)?;
            client.conn.send_event(
                false,
                self.requestor,
                EventMask::default(),
                SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time,
                    requestor: self.requestor,
                    selection: self.selection,
                    target: self.target,
                    property: self.property,
                },
            )?;
            client.conn.flush()?;
            return Ok(TransferResult::Done);
        } else if self.offset == usize::MAX {
            return self.send_incr_begin(client, time);
        }

        self.send_incr(client)
    }

    fn send_incr_begin(&mut self, client: &XClient, time: u32) -> Result<TransferResult> {
        log::debug!("send_incr_begin");
        self.offset = 0;
        // To subscribe the PropertyNotify event
        client.conn.change_window_attributes(
            self.requestor,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        client.conn.change_property32(
            PropMode::REPLACE,
            self.requestor,
            self.property,
            client.atoms.INCR,
            &[0],
        )?;
        client.conn.send_event(
            false,
            self.requestor,
            EventMask::default(),
            SelectionNotifyEvent {
                response_type: SELECTION_NOTIFY_EVENT,
                sequence: 0,
                time,
                requestor: self.requestor,
                selection: self.selection,
                target: self.target,
                property: self.property,
            },
        )?;
        client.conn.flush()?;
        Ok(TransferResult::Continue)
    }

    fn send_incr(&mut self, client: &XClient) -> Result<TransferResult> {
        log::debug!("send_incr");
        self.change_property_to_send(&client.conn)?;
        client.conn.flush()?;
        if self.offset > self.content.len() {
            log::debug!("send_incr finished");
            Ok(TransferResult::Done)
        } else {
            Ok(TransferResult::Continue)
        }
    }
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

fn mime_types_to_atoms(conn: &RustConnection, mime_types: &Vec<String>) -> Vec<u32> {
    let mut ret = vec![];
    for str in mime_types {
        match get_atom_id_by_name(conn, str) {
            Ok(atom) => ret.push(atom),
            Err(e) => {
                log::error!("Failed to convert {} into atom, {}", str, e)
            }
        }
    }

    ret
}

fn decide_mime_type_with_atom(
    conn: &RustConnection,
    prefered_atom: Atom,
    supported: &Vec<String>,
) -> Result<String> {
    let prefered = get_atom_name(conn, prefered_atom)?;
    let mime_type = decide_mime_type(&prefered, supported)?;
    Ok(mime_type)
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

pub fn copy_x<T: SourceData>(config: CopyConfig<T>) -> Result<()> {
    let mut state = XCopyState {
        source_data: &config.source_data,
        ongoing_senders: HashMap::new(),
    };
    let client = create_x_client(None)?;

    let selection = if config.use_primary {
        client.atoms.PRIMARY
    } else {
        client.atoms.CLIPBOARD
    };
    // Take over the clipboard
    // Xclip does a double check which doesn't seem to be necessary:
    // https://github.com/astrand/xclip/commit/33dc754c64c78ab0bd112b5bd34f7d517de76418
    client
        .conn
        .set_selection_owner(client.win_id, selection, CURRENT_TIME)
        .context("Failed to call set_selection_owner")?;
    client.conn.flush().context("Failed to flush connection")?;

    loop {
        let event = client
            .conn
            .wait_for_event()
            .context("Failed to get X event")?;
        match event {
            Event::SelectionRequest(event) => {
                log::debug!(
                    "Received SelectionRequest with target {} from requestor {}",
                    event.target,
                    event.requestor
                );
                if event.target == client.atoms.TARGETS {
                    // Ask for supported mime-types
                    // 'TARGETS' should always be the first supported target (mime-type)
                    let mut atoms = vec![client.atoms.TARGETS];
                    atoms.extend(mime_types_to_atoms(
                        &client.conn,
                        &state.source_data.mime_types(),
                    ));
                    // In theory, sending TARGETS could cause INCR transfer as well.
                    // However, that requires some complex generic handling for XSelectionSender
                    // which I failed to implement nicely.
                    client.conn.change_property32(
                        PropMode::REPLACE,
                        event.requestor,
                        event.property,
                        client.atoms.ATOM,
                        &atoms,
                    )?;
                    client.conn.send_event(
                        false,
                        event.requestor,
                        EventMask::default(),
                        SelectionNotifyEvent {
                            response_type: SELECTION_NOTIFY_EVENT,
                            sequence: 0,
                            time: event.time,
                            requestor: event.requestor,
                            selection: event.selection,
                            target: event.target,
                            property: event.property,
                        },
                    )?;
                    client.conn.flush()?;
                } else {
                    // Ask the content of the clipboard
                    let content = match decide_mime_type_with_atom(
                        &client.conn,
                        event.target,
                        &state.source_data.mime_types(),
                    ) {
                        Ok(mime_type_str) => {
                            state.source_data.content_by_mime_type(&mime_type_str).1
                        }
                        Err(e) => {
                            log::debug!(
                                "The requested target (mime-type) cannot be provided. {}",
                                e
                            );
                            // Cannot find content, reply empty
                            Rc::new(Vec::<u8>::new())
                        }
                    };
                    let mut sender = XSelectionSender::new(&client, &event, event.target, content);
                    if sender.send(&client, event.time)? == TransferResult::Continue {
                        state.ongoing_senders.insert(event.requestor, sender);
                    }
                }
            }
            Event::PropertyNotify(event) => {
                log::debug!(
                    "Received PropertyNotify from window {}, state {}",
                    event.window,
                    u8::from(event.state)
                );
                if event.state != Property::DELETE {
                    // DELETE means the other side is ready for the next chunk of data.
                    continue;
                };
                if let Some(sender) = state.ongoing_senders.get_mut(&event.window) {
                    if sender.send(&client, event.time)? == TransferResult::Done {
                        // INCR finished
                        state.ongoing_senders.remove(&event.window);
                    }
                } else {
                    // Should not happen
                    log::error!("Couldn't find the sender");
                }
            }
            Event::SelectionClear(_) => {
                log::debug!("Received SelectionClear");
                break;
            }
            event => {
                log::debug!("Unhandled event {event:?}");
                break;
            }
        }
    }
    Ok(())
}
