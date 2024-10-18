// A thread handles screencopy, and other wayland protocols, returning information as a
// subscription.

use calloop_wayland_source::WaylandSource;
use cctk::{
    screencopy::ScreencopyState,
    sctk::{
        self,
        dmabuf::{DmabufFeedback, DmabufState},
        registry::{ProvidesRegistryState, RegistryState},
        seat::{SeatHandler, SeatState},
        shm::{Shm, ShmHandler},
    },
    toplevel_info::ToplevelInfoState,
    toplevel_management::ToplevelManagerState,
    wayland_client::{
        globals::registry_queue_init, protocol::wl_seat, Connection, Proxy, QueueHandle,
    },
    workspace::WorkspaceState,
};
use cosmic::{
    cctk,
    iced::{
        self,
        futures::{executor::block_on, FutureExt, SinkExt},
    },
};
use futures_channel::mpsc;
use std::{cell::RefCell, collections::HashMap, fs, path::PathBuf, sync::Arc, thread};

mod buffer;
use buffer::Buffer;
mod capture;
use capture::{Capture, CaptureSource};
mod dmabuf;
mod screencopy;
use screencopy::{ScreencopySession, SessionData};
mod toplevel;
mod workspace;

use super::{CaptureFilter, CaptureImage, Cmd, Event};

pub fn subscription(conn: Connection) -> iced::Subscription<Event> {
    iced::Subscription::run_with_id("wayland-sub", async { start(conn) }.flatten_stream())
}

pub struct AppData {
    qh: QueueHandle<Self>,
    dmabuf_state: DmabufState,
    registry_state: RegistryState,
    toplevel_info_state: ToplevelInfoState,
    workspace_state: WorkspaceState,
    screencopy_state: ScreencopyState,
    seat_state: SeatState,
    shm_state: Shm,
    toplevel_manager_state: ToplevelManagerState,
    sender: mpsc::Sender<Event>,
    capture_filter: CaptureFilter,
    captures: RefCell<HashMap<CaptureSource, Arc<Capture>>>,
    dmabuf_feedback: Option<DmabufFeedback>,
    gbm: Option<(PathBuf, gbm::Device<fs::File>)>,
    scheduler: calloop::futures::Scheduler<()>,
}

impl AppData {
    fn send_event(&mut self, event: Event) {
        let _ = block_on(self.sender.send(event));
    }

    // Handle message from main thread
    fn handle_cmd(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::CaptureFilter(filter) => {
                self.capture_filter = filter;
                self.invalidate_capture_filter();
            }
            Cmd::ActivateToplevel(toplevel_handle) => {
                for seat in self.seat_state.seats() {
                    self.toplevel_manager_state
                        .manager
                        .activate(&toplevel_handle, &seat);
                }
            }
            Cmd::CloseToplevel(toplevel_handle) => {
                self.toplevel_manager_state.manager.close(&toplevel_handle);
            }
            Cmd::MoveToplevelToWorkspace(toplevel_handle, workspace_handle, output) => {
                if self.toplevel_manager_state.manager.version() >= 2 {
                    self.toplevel_manager_state.manager.move_to_workspace(
                        &toplevel_handle,
                        &workspace_handle,
                        &output,
                    );
                }
            }
            Cmd::ActivateWorkspace(workspace_handle) => {
                if let Ok(workspace_manager) = self.workspace_state.workspace_manager().get() {
                    workspace_handle.activate();
                    workspace_manager.commit();
                }
            }
        }
    }

    fn matches_capture_filter(&self, source: &CaptureSource) -> bool {
        match source {
            CaptureSource::Toplevel(toplevel) => {
                let info = self.toplevel_info_state.info(toplevel).unwrap();
                info.workspace.iter().any(|workspace| {
                    self.capture_filter
                        .toplevels_on_workspaces
                        .contains(workspace)
                })
            }
            CaptureSource::Workspace(_, output) => {
                self.capture_filter.workspaces_on_outputs.contains(output)
            }
        }
    }

    fn invalidate_capture_filter(&self) {
        for (source, capture) in self.captures.borrow_mut().iter_mut() {
            let matches = self.matches_capture_filter(source);
            if matches {
                capture.start(&self.screencopy_state, &self.qh);
            } else {
                capture.stop();
            }
        }
    }

    fn add_capture_source(&self, source: CaptureSource) {
        self.captures
            .borrow_mut()
            .entry(source.clone())
            .or_insert_with(|| {
                let matches = self.matches_capture_filter(&source);
                let capture = Capture::new(source);
                if matches {
                    capture.start(&self.screencopy_state, &self.qh);
                }
                capture
            });
    }

    fn remove_capture_source(&self, source: CaptureSource) {
        if let Some(capture) = self.captures.borrow_mut().remove(&source) {
            capture.stop();
        }
    }
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    sctk::registry_handlers!(SeatState);
}

impl SeatHandler for AppData {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: sctk::seat::Capability,
    ) {
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: sctk::seat::Capability,
    ) {
    }
}

impl ShmHandler for AppData {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

fn start(conn: Connection) -> mpsc::Receiver<Event> {
    let (sender, receiver) = mpsc::channel(20);

    let (globals, event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    let dmabuf_state = DmabufState::new(&globals, &qh);
    if let Err(err) = dmabuf_state.get_default_feedback(&qh) {
        log::warn!("dmabuf feedback not supported, only shm: {}", err);
    }

    thread::spawn(move || {
        let (executor, scheduler) = calloop::futures::executor().unwrap();

        let registry_state = RegistryState::new(&globals);
        let mut app_data = AppData {
            qh: qh.clone(),
            dmabuf_state,
            workspace_state: WorkspaceState::new(&registry_state, &qh), // Create before toplevel info state
            toplevel_info_state: ToplevelInfoState::new(&registry_state, &qh),
            toplevel_manager_state: ToplevelManagerState::new(&registry_state, &qh),
            screencopy_state: ScreencopyState::new(&globals, &qh),
            registry_state,
            seat_state: SeatState::new(&globals, &qh),
            shm_state: Shm::bind(&globals, &qh).unwrap(),
            sender,
            capture_filter: CaptureFilter::default(),
            captures: RefCell::new(HashMap::new()),
            dmabuf_feedback: None,
            gbm: None,
            scheduler,
        };

        let (cmd_sender, cmd_channel) = calloop::channel::channel();
        app_data.send_event(Event::CmdSender(cmd_sender));

        let mut event_loop = calloop::EventLoop::try_new().unwrap();
        WaylandSource::new(conn, event_queue)
            .insert(event_loop.handle())
            .unwrap();
        event_loop
            .handle()
            .insert_source(cmd_channel, |event, _, app_data| {
                if let calloop::channel::Event::Msg(msg) = event {
                    app_data.handle_cmd(msg)
                }
            })
            .unwrap();
        event_loop
            .handle()
            .insert_source(executor, |(), _, _| {})
            .unwrap();

        loop {
            event_loop.dispatch(None, &mut app_data).unwrap();
        }
    });

    receiver
}

// Don't bind outputs; use `WlOutput` instances from iced-sctk
sctk::delegate_registry!(AppData);
sctk::delegate_seat!(AppData);
sctk::delegate_shm!(AppData);
