#![cfg(target_os = "linux")]

use super::mime_type::decide_mime_type;
use super::ClipBackend;
use super::CopyConfig;
use super::PasteConfig;
use crate::protocol::SourceData;
use anyhow::{bail, Context, Error, Result};
use nix::unistd::{pipe, read};
use std::collections::HashMap;
use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use wayrs_client::global::GlobalExt;
use wayrs_client::protocol::wl_seat::WlSeat;
use wayrs_client::{Connection, EventCtx, IoMode};
use wayrs_protocols::wlr_data_control_unstable_v1::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
    ZwlrDataControlManagerV1,
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
    finishied: bool,
    result: Option<Error>,
    // Stored offers for selection and primary selection (middle-click paste).
    offers: HashMap<ZwlrDataControlOfferV1, Vec<String>>,

    config: PasteConfig,
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
    let (mut conn, globals) = Connection::<T>::connect_and_collect_globals()
        .context("Failed to create wayland connection")?;

    let mut seat_opt: Option<WlSeat> = None;
    for g in &globals {
        if g.is::<WlSeat>() {
            if seat_opt.is_none() {
                seat_opt = Some(g.bind(&mut conn, 2..=4).unwrap());
            } else {
                log::debug!("More than one WlSeat found, this is not expected")
            }
        }
    }
    let seat = seat_opt.context("Failed to find 'WlSeat'")?;

    let data_ctl_mgr: ZwlrDataControlManagerV1 = globals
        .iter()
        .find(|g| g.is::<ZwlrDataControlManagerV1>())
        .context(
            "No zwlr_data_control_manager_v1 global found, \
			ensure compositor supports wlr-data-control-unstable-v1 protocol",
        )?
        .bind(&mut conn, ..=2)
        .context("Failed to bind to the 'ZwlrDataControlManagerV1'")?;

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
        finishied: false,
        result: None,
        offers: HashMap::new(),
        config: cfg,
    };

    client.conn.flush(IoMode::Blocking).unwrap();
    loop {
        if state.finishied {
            break;
        }
        client.conn.recv_events(IoMode::Blocking).unwrap();
        client.conn.dispatch_events(&mut state);
    }

    if state.result.is_none() {
        return Ok(());
    }

    bail!(state.result.unwrap());
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
    macro_rules! unwrap_or_return {
        ( $e:expr, $report_error:expr) => {
            match $e {
                Ok(x) => x,
                Err(e) => {
                    if $report_error {
                        ctx.state.result = Some(e.into())
                    } else {
                        // Errors like empty clipboard are not real problems
                        log::error!("{}", e)
                    }
                    ctx.state.finishied = true;
                    ctx.conn.break_dispatch_loop();
                    return;
                }
            }
        };
    }

    match ctx.event {
        // Received before Selection or PrimarySelection
        // Need to request mime-types here
        zwlr_data_control_device_v1::Event::DataOffer(offer) => {
            if ctx.state.offers.insert(offer, Vec::new()).is_some() {
                log::error!("Duplicated offer received")
            }
            ctx.conn.set_callback_for(offer, move |ctx| {
                if let zwlr_data_control_offer_v1::Event::Offer(mime_type) = ctx.event {
                    if let Ok(str) = mime_type.to_str() {
                        let new_type = str.to_string();
                        let mime_types = ctx.state.offers.get_mut(&offer).unwrap();
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
        // Do paste here
        zwlr_data_control_device_v1::Event::PrimarySelection(o)
        | zwlr_data_control_device_v1::Event::Selection(o) => {
            match ctx.event {
                zwlr_data_control_device_v1::Event::PrimarySelection(_) => {
                    if !ctx.state.config.use_primary {
                        return;
                    }
                }
                _ => {
                    if ctx.state.config.use_primary {
                        return;
                    }
                }
            }
            if o.is_none() {
                log::error!("No data in the clipboard");
                ctx.state.finishied = true;
                ctx.conn.break_dispatch_loop();
                return;
            }
            let obj_id = o.unwrap();

            let (offer, supported_types) = ctx
                .state
                .offers
                .iter()
                .find(|pair| *(pair.0) == obj_id)
                .unwrap();

            // with "-l", list the mime-types and return
            if ctx.state.config.list_types_only {
                for mt in supported_types {
                    writeln!(ctx.state.config.writter, "{}", mt).unwrap()
                }
                ctx.state.finishied = true;
                ctx.conn.break_dispatch_loop();
                return;
            }

            let str = unwrap_or_return!(
                decide_mime_type(&ctx.state.config.expected_mime_type, supported_types),
                false
            );
            let mime_type = unwrap_or_return!(CString::new(str), true);

            // offer.receive needs a fd to write, we cannot use the stdin since the read side of the
            // pipe may close earlier before all data written.
            let fds = unwrap_or_return!(pipe(), true);
            offer.receive(ctx.conn, mime_type, fds.1);
            // This looks strange, but it is working. It seems offer.receive is a request but nont a
            // blocking call, which needs an extra loop to finish. Maybe a callback needs to be set
            // to wait until it is processed, but I have no idea how to do that.
            // conn.set_callback_for() doesn't work for the offer here.
            ctx.conn.blocking_roundtrip().unwrap();
            let mut buffer = vec![0; 1024 * 4];
            loop {
                // Read from the pipe until EOF
                let n = unwrap_or_return!(read(fds.0.as_raw_fd(), &mut buffer), true);
                if n > 0 {
                    // Write the content to the destination
                    unwrap_or_return!(ctx.state.config.writter.write(&buffer[0..n]), true);
                } else {
                    break;
                }
            }

            offer.destroy(ctx.conn);
            ctx.state.finishied = true;
            ctx.conn.break_dispatch_loop();
        }
        zwlr_data_control_device_v1::Event::Finished => {
            log::debug!("Received 'Finished' event");
            ctx.state.result = Some(Error::msg("The data control object has been destroyed"));
            ctx.state.finishied = true;
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
