// A thread handles screencopy, and other wayland protocols, returning information as a
// subscription.

use calloop_wayland_source::WaylandSource;
use cctk::{
    cosmic_protocols::workspace::v2::client::zcosmic_workspace_handle_v2,
    screencopy::{CaptureSource, ScreencopyState},
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
        Connection, Proxy, QueueHandle, globals::registry_queue_init, protocol::wl_seat,
    },
    workspace::WorkspaceState,
};
use cosmic::{
    cctk,
    iced::{
        self,
        futures::{
            FutureExt, SinkExt,
            channel::mpsc,
            executor::{ThreadPool, block_on},
        },
    },
};
use std::{cell::RefCell, collections::HashMap, sync::Arc, thread};

mod buffer;
use buffer::Buffer;
mod capture;
use capture::Capture;
mod dmabuf;
mod gbm_devices;
use gbm_devices::GbmDevices;
mod screencopy;
use screencopy::{ScreencopySession, SessionData};
mod toplevel;
mod vulkan;
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
    toplevel_manager_state: Option<ToplevelManagerState>,
    sender: mpsc::Sender<Event>,
    capture_filter: CaptureFilter,
    captures: RefCell<HashMap<CaptureSource, Arc<Capture>>>,
    dmabuf_feedback: Option<DmabufFeedback>,
    gbm_devices: GbmDevices,
    thread_pool: ThreadPool,
    vulkan: Option<vulkan::Vulkan>,
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
                let info = self.toplevel_info_state.info(&toplevel_handle);
                if let Some(cosmic_toplevel) = info.and_then(|x| x.cosmic_toplevel.as_ref()) {
                    for seat in self.seat_state.seats() {
                        if let Some(state) = &self.toplevel_manager_state {
                            state.manager.activate(cosmic_toplevel, &seat);
                        }
                    }
                }
            }
            Cmd::CloseToplevel(toplevel_handle) => {
                let info = self.toplevel_info_state.info(&toplevel_handle);
                if let Some(cosmic_toplevel) = info.and_then(|x| x.cosmic_toplevel.as_ref())
                    && let Some(state) = &self.toplevel_manager_state
                {
                    state.manager.close(cosmic_toplevel);
                }
            }
            Cmd::MoveToplevelToWorkspace(toplevel_handle, workspace_handle, output) => {
                let info = self.toplevel_info_state.info(&toplevel_handle);
                if let Some(cosmic_toplevel) = info.and_then(|x| x.cosmic_toplevel.as_ref())
                    && let Some(state) = &self.toplevel_manager_state
                    && state.manager.version() >= 2
                {
                    state.manager.move_to_ext_workspace(
                        cosmic_toplevel,
                        &workspace_handle,
                        &output,
                    );
                }
            }
            // TODO version check
            Cmd::MoveWorkspaceBefore(workspace_handle, other_workspace_handle) => {
                if let Ok(workspace_manager) = self.workspace_state.workspace_manager().get()
                    && let Some(cosmic_workspace) = self
                        .workspace_state
                        .workspaces()
                        .find(|w| w.handle == workspace_handle)
                        .and_then(|w| w.cosmic_handle.as_ref())
                    && cosmic_workspace.version()
                        >= zcosmic_workspace_handle_v2::REQ_MOVE_BEFORE_SINCE
                {
                    cosmic_workspace.move_before(&other_workspace_handle, 0);
                    workspace_manager.commit();
                }
            }
            Cmd::MoveWorkspaceAfter(workspace_handle, other_workspace_handle) => {
                if let Ok(workspace_manager) = self.workspace_state.workspace_manager().get()
                    && let Some(cosmic_workspace) = self
                        .workspace_state
                        .workspaces()
                        .find(|w| w.handle == workspace_handle)
                        .and_then(|w| w.cosmic_handle.as_ref())
                    && cosmic_workspace.version()
                        >= zcosmic_workspace_handle_v2::REQ_MOVE_AFTER_SINCE
                {
                    cosmic_workspace.move_after(&other_workspace_handle, 0);
                    workspace_manager.commit();
                }
            }
            Cmd::ActivateWorkspace(workspace_handle) => {
                if let Ok(workspace_manager) = self.workspace_state.workspace_manager().get() {
                    workspace_handle.activate();
                    workspace_manager.commit();
                }
            }
            Cmd::SetWorkspacePinned(workspace_handle, pinned) => {
                if let Ok(workspace_manager) = self.workspace_state.workspace_manager().get()
                    && let Some(cosmic_workspace) = self
                        .workspace_state
                        .workspaces()
                        .find(|w| w.handle == workspace_handle)
                        .and_then(|w| w.cosmic_handle.as_ref())
                    && cosmic_workspace.version() >= zcosmic_workspace_handle_v2::REQ_PIN_SINCE
                {
                    // TODO check capability
                    if pinned {
                        cosmic_workspace.pin();
                    } else {
                        cosmic_workspace.unpin();
                    }
                    workspace_manager.commit();
                }
            }
        }
    }

    fn matches_capture_filter(&self, source: &CaptureSource) -> bool {
        match source {
            CaptureSource::Toplevel(toplevel) => {
                let info = self
                    .toplevel_info_state
                    .toplevels()
                    .find(|info| info.foreign_toplevel == *toplevel);
                if let Some(info) = info {
                    self.capture_filter.toplevel_matches(info)
                } else {
                    false
                }
            }
            CaptureSource::Workspace(workspace) => self
                .workspace_state
                .workspace_groups()
                .find(|g| g.workspaces.iter().any(|w| w == workspace))
                .is_some_and(|group| {
                    self.capture_filter
                        .workspace_outputs_matches(&group.outputs)
                }),
            CaptureSource::Output(_) => false,
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
        // TODO: The `calloop` executor doesn't seem to be working properly, so
        // spawn an executor using one additional thread.
        let thread_pool = ThreadPool::builder().pool_size(1).create().unwrap();

        let registry_state = RegistryState::new(&globals);
        let mut app_data = AppData {
            qh: qh.clone(),
            dmabuf_state,
            workspace_state: WorkspaceState::new(&registry_state, &qh), // Create before toplevel info state
            toplevel_info_state: ToplevelInfoState::new(&registry_state, &qh),
            toplevel_manager_state: ToplevelManagerState::try_new(&registry_state, &qh),
            screencopy_state: ScreencopyState::new(&globals, &qh),
            registry_state,
            seat_state: SeatState::new(&globals, &qh),
            shm_state: Shm::bind(&globals, &qh).unwrap(),
            sender,
            capture_filter: CaptureFilter::default(),
            captures: RefCell::new(HashMap::new()),
            dmabuf_feedback: None,
            gbm_devices: GbmDevices::default(),
            thread_pool,
            vulkan: vulkan::Vulkan::new(),
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
