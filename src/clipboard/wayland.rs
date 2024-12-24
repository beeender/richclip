use super::mime_type::decide_mime_type;
use super::PasteConfig;
use crate::source_data::SourceData;
use anyhow::{Context, Error, Result};
use std::collections::HashMap;
use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsFd;
use wayrs_client::global::GlobalExt;
use wayrs_client::protocol::wl_seat::WlSeat;
use wayrs_client::{Connection, EventCtx, IoMode};
use wayrs_protocols::wlr_data_control_unstable_v1::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
    ZwlrDataControlManagerV1,
};

struct WaylandClient<T> {
    conn: Connection<T>,
    seat: WlSeat,
    data_ctl_mgr: ZwlrDataControlManagerV1,
}

struct CopyEventState<'a> {
    finishied: bool,
    result: Option<Error>,
    source_data: &'a dyn SourceData,
}

struct PasteEventState<'a, T: AsFd + Write> {
    finishied: bool,
    result: Option<Error>,
    // Stored offers for selection and primary selection (middle-click paste).
    offers: HashMap<ZwlrDataControlOfferV1, Vec<String>>,

    config: PasteConfig<'a, T>,
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

pub fn paste_wayland<T: AsFd + Write + 'static>(cfg: PasteConfig<T>) -> Result<()> {
    let mut client =
        create_wayland_client::<PasteEventState<T>>().context("Faild to create wayland client")?;

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
    Ok(())
}

pub fn copy_wayland(source_data: impl SourceData) -> Result<()> {
    let mut client =
        create_wayland_client::<CopyEventState>().context("Faild to create wayland client")?;

    let source = client
        .data_ctl_mgr
        .create_data_source_with_cb(&mut client.conn, wl_source_cb);
    source_data.mime_types().iter().for_each(|mime| {
        let cstr = CString::new(mime.as_bytes()).unwrap();
        source.offer(&mut client.conn, cstr);
    });

    let data_control_device = client
        .data_ctl_mgr
        .get_data_device(&mut client.conn, client.seat);
    data_control_device.set_selection(&mut client.conn, Some(source));

    let mut state = CopyEventState {
        finishied: false,
        result: None,
        source_data: &source_data,
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
fn wl_device_cb_for_paste<T: AsFd + Write>(
    ctx: EventCtx<PasteEventState<T>, ZwlrDataControlDeviceV1>,
) {
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
                    let str = mime_type.to_str();
                    if str.is_err() {
                        log::error!("Failed to convert '{:x?}' to String", mime_type.as_bytes());
                    } else {
                        let mime_types = ctx.state.offers.get_mut(&offer).unwrap();
                        mime_types.push(str.unwrap().to_string());
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

            let fd = unwrap_or_return!(
                ctx.state.config.fd_to_write.as_fd().try_clone_to_owned(),
                true
            );
            let (offer, supported_types) = ctx
                .state
                .offers
                .iter()
                .find(|pair| *(pair.0) == obj_id)
                .unwrap();

            // with "-l", list the mime-types and return
            if ctx.state.config.list_types_only {
                for mt in supported_types {
                    writeln!(ctx.state.config.fd_to_write, "{}", mt).unwrap()
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

            offer.receive(ctx.conn, mime_type, fd);
            ctx.conn.flush(IoMode::Blocking).unwrap();
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

fn wl_source_cb(ctx: EventCtx<CopyEventState, ZwlrDataControlSourceV1>) {
    match ctx.event {
        zwlr_data_control_source_v1::Event::Send(zwlr_data_control_source_v1::SendArgs {
            mime_type,
            fd,
        }) => {
            let src_data = ctx.state.source_data;
            let mut file = File::from(fd);
            let content = src_data
                .content_by_mime_type(mime_type.to_str().unwrap())
                .unwrap();
            file.write_all(content).unwrap();
        }
        zwlr_data_control_source_v1::Event::Cancelled => {
            ctx.conn.break_dispatch_loop();
            ctx.state.finishied = true;
        }
        _ => {}
    }
}
