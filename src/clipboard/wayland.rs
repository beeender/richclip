use super::ClipBackend;
use super::CopyConfig;
use super::PasteConfig;
use super::mime_type::decide_mime_type;
use crate::protocol::SourceData;
use anyhow::{Context, Error, Result};
use nix::unistd::{pipe, read};
use std::collections::HashMap;
use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use wayrs_client::core::ObjectId;
use wayrs_client::protocol::wl_seat::WlSeat;
use wayrs_client::{Connection, EventCtx, IoMode};
use wayrs_protocols::wlr_data_control_unstable_v1::{
    ZwlrDataControlManagerV1,
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
};

pub struct WaylandBackend {}

struct WaylandClient<T> {
    conn: Connection<T>,
    seat: WlSeat,
    data_ctl_mgr: ZwlrDataControlManagerV1,
}

struct CopyEventState {
    finishied: bool,
    source_data: Box<dyn SourceData>,
}

struct PasteEventState {
    // Stored offers for selection and primary selection (middle-click paste).
    offers: HashMap<ZwlrDataControlOfferV1, Vec<String>>,
    stage: PasteEventStage,

    config: PasteConfig,
}

enum PasteEventStage {
    Done,
    Err(Error),
    CollectingOffers,
    GotSelection(ObjectId),
}

impl ClipBackend for WaylandBackend {
    fn copy(&self, config: CopyConfig) -> Result<()> {
        copy_wayland(config)
    }

    fn paste(&self, config: PasteConfig) -> Result<()> {
        paste_wayland(config)
    }
}

fn create_wayland_client<T>() -> Result<WaylandClient<T>> {
    let mut conn = Connection::<T>::connect().context("Failed to create wayland connection")?;
    conn.blocking_roundtrip()
        .context("Failed to call 'blocking_roundtrip'")?;

    let seat: WlSeat = conn.bind_singleton(2..=4).context("")?;
    let data_ctl_mgr: ZwlrDataControlManagerV1 = conn.bind_singleton(..=2).context("")?;

    Ok(WaylandClient::<T> {
        conn,
        seat,
        data_ctl_mgr,
    })
}

fn paste_wayland(cfg: PasteConfig) -> Result<()> {
    let mut client =
        create_wayland_client::<PasteEventState>().context("Faild to create wayland client")?;

    let _data_control_device = client.data_ctl_mgr.get_data_device_with_cb(
        &mut client.conn,
        client.seat,
        wl_device_cb_for_paste,
    );

    let mut state = PasteEventState {
        offers: HashMap::new(),
        stage: PasteEventStage::CollectingOffers,
        config: cfg,
    };

    let selection_id = loop {
        match state.stage {
            PasteEventStage::Done => return Ok(()),
            PasteEventStage::Err(err) => return Err(err),
            PasteEventStage::CollectingOffers => (),
            PasteEventStage::GotSelection(id) => break id,
        }

        client.conn.flush(IoMode::Blocking).unwrap();
        client.conn.recv_events(IoMode::Blocking).unwrap();
        client.conn.dispatch_events(&mut state);
    };

    let (offer, supported_types) = state.offers.get_key_value(&selection_id).unwrap();

    // with "-l", list the mime-types and return
    if state.config.list_types_only {
        for mt in supported_types {
            writeln!(state.config.writter, "{mt}")?;
        }
        return Ok(());
    }

    let mime_type = CString::new(decide_mime_type(
        &state.config.expected_mime_type,
        supported_types,
    )?)?;

    // offer.receive needs a fd to write, we cannot use the stdin since the read side of the
    // pipe may close earlier before all data written.
    let fds = pipe()?;
    offer.receive(&mut client.conn, mime_type, fds.1);
    client.conn.flush(IoMode::Blocking)?;

    let mut buffer = vec![0; 1024 * 4];
    loop {
        // Read from the pipe until EOF
        let n = read(fds.0.as_raw_fd(), &mut buffer)?;
        if n > 0 {
            // Write the content to the destination
            state.config.writter.write(&buffer[0..n])?;
        } else {
            break;
        }
    }

    Ok(())
}

fn copy_wayland(config: CopyConfig) -> Result<()> {
    let mut client =
        create_wayland_client::<CopyEventState>().context("Faild to create wayland client")?;

    let source = client
        .data_ctl_mgr
        .create_data_source_with_cb(&mut client.conn, wl_source_cb_for_copy);
    config.source_data.mime_types().iter().for_each(|mime| {
        let cstr = CString::new(mime.as_bytes()).unwrap();
        source.offer(&mut client.conn, cstr);
    });

    let data_control_device = client
        .data_ctl_mgr
        .get_data_device(&mut client.conn, client.seat);
    if config.use_primary {
        data_control_device.set_primary_selection(&mut client.conn, Some(source));
    } else {
        data_control_device.set_selection(&mut client.conn, Some(source));
    }

    let mut state = CopyEventState {
        finishied: false,
        source_data: config.source_data,
    };

    client.conn.flush(IoMode::Blocking).unwrap();
    loop {
        if state.finishied {
            break;
        }
        client.conn.recv_events(IoMode::Blocking).unwrap();
        client.conn.dispatch_events(&mut state);
    }

    Ok(())
}

#[allow(clippy::collapsible_match)]
fn wl_device_cb_for_paste(ctx: EventCtx<PasteEventState, ZwlrDataControlDeviceV1>) {
    match ctx.event {
        // Received before Selection or PrimarySelection
        // Need to request mime-types here
        zwlr_data_control_device_v1::Event::DataOffer(offer) => {
            if ctx.state.offers.insert(offer, Vec::new()).is_some() {
                log::error!("Duplicated offer received")
            }
            ctx.conn.set_callback_for(offer, |ctx| {
                if let zwlr_data_control_offer_v1::Event::Offer(mime_type) = ctx.event {
                    if let Ok(str) = mime_type.to_str() {
                        let new_type = str.to_string();
                        let mime_types = ctx.state.offers.get_mut(&ctx.proxy).unwrap();
                        if !mime_types.iter().any(|s| new_type.eq(s)) {
                            // Duplicated mime-types could be reported (wl-paste -l shows the same)
                            mime_types.push(new_type);
                        }
                    } else {
                        log::error!("Failed to convert '{:x?}' to String", mime_type.as_bytes());
                    }
                }
            });
        }
        zwlr_data_control_device_v1::Event::Selection(o) => {
            if !ctx.state.config.use_primary {
                let Some(obj_id) = o else {
                    log::error!("No data in the clipboard");
                    ctx.state.stage = PasteEventStage::Done;
                    ctx.conn.break_dispatch_loop();
                    return;
                };
                ctx.state.stage = PasteEventStage::GotSelection(obj_id);
            }
        }
        zwlr_data_control_device_v1::Event::PrimarySelection(o) => {
            if ctx.state.config.use_primary {
                let Some(obj_id) = o else {
                    log::error!("No data in the clipboard");
                    ctx.state.stage = PasteEventStage::Done;
                    ctx.conn.break_dispatch_loop();
                    return;
                };
                ctx.state.stage = PasteEventStage::GotSelection(obj_id);
            }
        }
        zwlr_data_control_device_v1::Event::Finished => {
            log::debug!("Received 'Finished' event");
            ctx.state.stage =
                PasteEventStage::Err(Error::msg("The data control object has been destroyed"));
            ctx.conn.break_dispatch_loop();
        }
        _ => unreachable!("Unexpected event for device callback"),
    }
}

fn wl_source_cb_for_copy(ctx: EventCtx<CopyEventState, ZwlrDataControlSourceV1>) {
    match ctx.event {
        zwlr_data_control_source_v1::Event::Send(zwlr_data_control_source_v1::SendArgs {
            mime_type,
            fd,
        }) => {
            log::debug!("Received 'Send' event");
            let src_data = &ctx.state.source_data;
            let mut file = File::from(fd);
            let (_, content) = src_data.content_by_mime_type(mime_type.to_str().unwrap());
            file.write_all(&content).unwrap();
        }
        zwlr_data_control_source_v1::Event::Cancelled => {
            log::debug!("Received 'Cancelled' event");
            ctx.conn.break_dispatch_loop();
            ctx.state.finishied = true;
        }
        _ => unreachable!("Unexpected event for source callback"),
    }
}
