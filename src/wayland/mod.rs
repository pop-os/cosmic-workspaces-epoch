// Workspaces Info, Toplevel Info
// Capture
// - subscribe to all workspaces, to start with? All that are associated with an output should be
// shown on one.
//   * Need output name to compare?

// TODO: Way to activate workspace, toplevel? Close? Move?

use cctk::{
    cosmic_protocols::{
        toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
        toplevel_management::v1::client::zcosmic_toplevel_manager_v1,
        workspace::v1::client::{zcosmic_workspace_handle_v1, zcosmic_workspace_manager_v1},
    },
    screencopy::ScreencopyState,
    sctk::{
        self,
        dmabuf::DmabufState,
        output::{OutputHandler, OutputState},
        reexports::calloop_wayland_source::WaylandSource,
        registry::{ProvidesRegistryState, RegistryState},
        seat::{SeatHandler, SeatState},
        shm::{Shm, ShmHandler},
    },
    toplevel_info::{ToplevelInfo, ToplevelInfoState},
    toplevel_management::ToplevelManagerState,
    wayland_client::{
        backend::ObjectId,
        globals::registry_queue_init,
        protocol::{wl_buffer, wl_output, wl_seat},
        Connection, Proxy, QueueHandle,
    },
    workspace::WorkspaceState,
};
use cosmic::iced::{
    self,
    futures::{executor::block_on, FutureExt, SinkExt},
    widget::image,
};
// use futures_channel::mpsc;
use crate::mpsc;
use std::{cell::RefCell, collections::HashMap, sync::Arc, thread};

mod buffer;
use buffer::Buffer;
mod capture;
use capture::{Capture, CaptureSource};
mod dmabuf;
mod screencopy;
mod toplevel;
mod workspace;

pub use capture::CaptureFilter;

// TODO define subscription for a particular output/workspace/toplevel (but we want to rate limit?)

#[derive(Clone, Debug)]
pub enum Event {
    CmdSender(calloop::channel::Sender<Cmd>),
    Connection(Connection),
    ToplevelManager(zcosmic_toplevel_manager_v1::ZcosmicToplevelManagerV1),
    WorkspaceManager(zcosmic_workspace_manager_v1::ZcosmicWorkspaceManagerV1),
    // XXX Output name rather than `WlOutput`
    Workspaces(Vec<(Vec<String>, cctk::workspace::Workspace)>),
    WorkspaceCapture(
        zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
        String,
        //image::Handle,
        wl_buffer::WlBuffer,
    ),
    NewToplevel(
        zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
        Option<String>,
        ToplevelInfo,
    ),
    UpdateToplevel(
        zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
        Option<String>,
        ToplevelInfo,
    ),
    CloseToplevel(zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1),
    ToplevelCapture(
        zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
        //image::Handle,
        wl_buffer::WlBuffer,
    ),
    Seats(Vec<wl_seat::WlSeat>),
}

/*
pub fn subscription() -> iced::Subscription<Event> {
    iced::subscription::run_with_id("wayland-sub", async { start() }.flatten_stream())
}
*/

#[derive(Debug)]
pub enum Cmd {
    CaptureFilter(CaptureFilter),
}

pub struct AppData {
    qh: QueueHandle<Self>,
    dmabuf_state: DmabufState,
    output_state: OutputState,
    registry_state: RegistryState,
    toplevel_info_state: ToplevelInfoState,
    workspace_state: WorkspaceState,
    screencopy_state: ScreencopyState,
    seat_state: SeatState,
    shm_state: Shm,
    toplevel_manager_state: ToplevelManagerState,
    sender: mpsc::Sender<Event>,
    output_names: HashMap<ObjectId, Option<String>>,
    seats: Vec<wl_seat::WlSeat>,
    capture_filter: CaptureFilter,
    captures: RefCell<HashMap<CaptureSource, Arc<Capture>>>,
}

impl AppData {
    fn send_event(&mut self, event: Event) {
        let _ = self.sender.send(event);
    }

    // Handle message from main thread
    fn handle_cmd(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::CaptureFilter(filter) => {
                self.capture_filter = filter;
                self.invalidate_capture_filter();
            }
        }
    }

    fn matches_capture_filter(&self, source: &CaptureSource) -> bool {
        match source {
            CaptureSource::Toplevel(toplevel) => {
                let info = self.toplevel_info_state.info(toplevel).unwrap();
                if let Some(workspace) = &info.workspace {
                    self.capture_filter
                        .toplevels_on_workspaces
                        .contains(workspace)
                } else {
                    false
                }
            }
            CaptureSource::Workspace(_, output) => {
                if let Some(name) = &self.output_state.info(&output).and_then(|x| x.name) {
                    self.capture_filter.workspaces_on_outputs.contains(name)
                } else {
                    false
                }
            }
        }
    }

    fn invalidate_capture_filter(&self) {
        for (source, capture) in self.captures.borrow_mut().iter_mut() {
            let matches = self.matches_capture_filter(source);
            let running = capture.running();
            if running && !matches {
                capture.cancel();
            } else if !running & matches {
                capture.capture(&self.screencopy_state.screencopy_manager, &self.qh);
            }
        }
    }

    fn add_capture_source(&self, source: CaptureSource) {
        self.captures
            .borrow_mut()
            .entry(source.clone())
            .or_insert_with(|| {
                let matches = self.matches_capture_filter(&source);
                let capture = Arc::new(Capture::new(source));
                if matches {
                    capture.capture(&self.screencopy_state.screencopy_manager, &self.qh);
                }
                capture
            });
    }

    fn remove_capture_source(&self, source: CaptureSource) {
        if let Some(capture) = self.captures.borrow_mut().remove(&source) {
            capture.cancel();
        }
    }
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    sctk::registry_handlers!(OutputState, SeatState);
}

impl SeatHandler for AppData {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.seats.push(seat);
        self.send_event(Event::Seats(self.seats.clone()));
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if let Some(idx) = self.seats.iter().position(|i| i == &seat) {
            self.seats.remove(idx);
        }
        self.send_event(Event::Seats(self.seats.clone()));
    }

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

// TODO: don't need this if we use same connection with same IDs? Or?
impl OutputHandler for AppData {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let name = self.output_state.info(&output).unwrap().name;
        self.output_names.insert(output.id(), name);
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.output_names.remove(&output.id());
    }
}

// XXX
pub fn start(conn: Connection) -> mpsc::Receiver<Event> {
    let (sender, receiver) = mpsc::channel(20);

    // TODO share connection? Can't use same `WlOutput` with seperate connection
    //let conn = Connection::connect_to_env().unwrap();
    let (globals, event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    let dmabuf_state = DmabufState::new(&globals, &qh);

    let registry_state = RegistryState::new(&globals);
    let mut app_data = AppData {
        qh: qh.clone(),
        dmabuf_state,
        output_state: OutputState::new(&globals, &qh),
        workspace_state: WorkspaceState::new(&registry_state, &qh), // Create before toplevel info state
        toplevel_info_state: ToplevelInfoState::new(&registry_state, &qh),
        toplevel_manager_state: ToplevelManagerState::new(&registry_state, &qh),
        screencopy_state: ScreencopyState::new(&globals, &qh),
        registry_state,
        seat_state: SeatState::new(&globals, &qh),
        shm_state: Shm::bind(&globals, &qh).unwrap(),
        sender,
        output_names: HashMap::new(),
        seats: Vec::new(),
        capture_filter: CaptureFilter::default(),
        captures: RefCell::new(HashMap::new()),
    };

    app_data.send_event(Event::Connection(conn.clone()));
    app_data.send_event(Event::Seats(app_data.seat_state.seats().collect()));
    app_data.send_event(Event::ToplevelManager(
        app_data.toplevel_manager_state.manager.clone(),
    ));
    if let Ok(manager) = app_data.workspace_state.workspace_manager().get() {
        app_data.send_event(Event::WorkspaceManager(manager.clone()));
    }

    // XXX also monitor cmd sender? Use calloop?
    thread::spawn(move || {
        //event_queue.blocking_dispatch(&mut app_data).unwrap();

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

sctk::delegate_output!(AppData);
sctk::delegate_registry!(AppData);
sctk::delegate_seat!(AppData);
sctk::delegate_shm!(AppData);

// TODO create layer shell surface for each output
// - subsurfaces for each workspace; window on current workspace. filter update logic.
//   * still use iced, but hack away window creation, and make that send commad for layer shell? Or
//   easier no to?
// - seperate state for layer shell?
