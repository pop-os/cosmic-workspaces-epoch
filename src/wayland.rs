// Workspaces Info, Toplevel Info
// Capture
// - subscribe to all workspaces, to start with? All that are associated with an output should be
// shown on one.
//   * Need output name to compare?

// TODO: Way to activate workspace, toplevel? Close? Move?

use cctk::{
    cosmic_protocols::{
        screencopy::v1::client::{zcosmic_screencopy_manager_v1, zcosmic_screencopy_session_v1},
        toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
        toplevel_management::v1::client::zcosmic_toplevel_manager_v1,
        workspace::v1::client::{zcosmic_workspace_handle_v1, zcosmic_workspace_manager_v1},
    },
    screencopy::{
        BufferInfo, ScreencopyHandler, ScreencopySessionData, ScreencopySessionDataExt,
        ScreencopyState,
    },
    sctk::{
        self,
        event_loop::WaylandSource,
        globals::ProvidesBoundGlobal,
        output::{OutputHandler, OutputState},
        registry::{ProvidesRegistryState, RegistryState},
        seat::{SeatHandler, SeatState},
        shm::{raw::RawPool, ShmHandler, ShmState},
    },
    toplevel_info::{ToplevelInfo, ToplevelInfoHandler, ToplevelInfoState},
    toplevel_management::{ToplevelManagerHandler, ToplevelManagerState},
    wayland_client::{
        backend::ObjectId,
        globals::registry_queue_init,
        protocol::{wl_buffer, wl_output, wl_seat, wl_shm},
        Connection, Dispatch, Proxy, QueueHandle, WEnum,
    },
    workspace::{WorkspaceHandler, WorkspaceState},
};
use cosmic::iced::{
    self,
    futures::{executor::block_on, FutureExt, SinkExt},
    widget::image,
};
use futures_channel::mpsc;
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};

// TODO define subscription for a particular output/workspace/toplevel (but we want to rate limit?)

#[derive(Clone, Debug)]
pub enum Event {
    CmdSender(calloop::channel::Sender<Cmd>),
    Connection(Connection),
    ToplevelManager(zcosmic_toplevel_manager_v1::ZcosmicToplevelManagerV1),
    WorkspaceManager(zcosmic_workspace_manager_v1::ZcosmicWorkspaceManagerV1),
    // XXX Output name rather than `WlOutput`
    Workspaces(Vec<(Option<String>, cctk::workspace::Workspace)>),
    WorkspaceCapture(
        zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
        image::Handle,
    ),
    NewToplevel(
        zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
        ToplevelInfo,
    ),
    CloseToplevel(zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1),
    ToplevelCapture(
        zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
        image::Handle,
    ),
    Seats(Vec<wl_seat::WlSeat>),
}

pub fn subscription() -> iced::Subscription<Event> {
    iced::subscription::run("wayland-sub", async { start() }.flatten_stream())
}

#[derive(Clone, PartialEq, Eq)]
enum CaptureSource {
    Toplevel(zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1),
    Workspace(
        zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
        wl_output::WlOutput,
    ),
}

impl std::hash::Hash for CaptureSource {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        match self {
            Self::Toplevel(handle) => handle.id(),
            Self::Workspace(handle, output) => handle.id(),
        }
        .hash(state)
    }
}

#[derive(Clone, Debug, Default)]
pub struct CaptureFilter {
    pub workspaces_on_outputs: Vec<wl_output::WlOutput>,
    pub toplevels_on_workspaces: Vec<zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1>,
}

#[derive(Debug)]
pub enum Cmd {
    CaptureFilter(CaptureFilter),
}

struct Buffer {
    pool: RawPool,
    buffer: wl_buffer::WlBuffer,
    buffer_info: BufferInfo,
}

impl Buffer {
    fn new(
        buffer_info: BufferInfo,
        shm: &impl ProvidesBoundGlobal<wl_shm::WlShm, 1>,
        qh: &QueueHandle<AppData>,
    ) -> Self {
        // Assume format is already known to be valid
        let mut pool =
            RawPool::new((buffer_info.stride * buffer_info.height) as usize, shm).unwrap();
        let format = wl_shm::Format::try_from(buffer_info.format).unwrap();
        let buffer = pool.create_buffer(
            0,
            buffer_info.width as i32,
            buffer_info.height as i32,
            buffer_info.stride as i32,
            format,
            (),
            qh,
        );
        Self {
            pool,
            buffer,
            buffer_info,
        }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        self.buffer.destroy();
    }
}

struct Capture {
    buffer: Mutex<Option<Buffer>>,
    source: CaptureSource,
    first_frame: AtomicBool,
    cancelled: AtomicBool,
}

impl Capture {
    fn new(source: CaptureSource) -> Capture {
        Capture {
            buffer: Mutex::new(None),
            source,
            first_frame: AtomicBool::new(true),
            cancelled: AtomicBool::new(false),
        }
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn capture(
        self: &Arc<Self>,
        manager: &zcosmic_screencopy_manager_v1::ZcosmicScreencopyManagerV1,
        qh: &QueueHandle<AppData>,
    ) {
        let udata = SessionData {
            session_data: Default::default(),
            capture: self.clone(),
        };
        match &self.source {
            CaptureSource::Toplevel(toplevel) => {
                manager.capture_toplevel(
                    toplevel,
                    zcosmic_screencopy_manager_v1::CursorMode::Hidden,
                    &qh,
                    udata,
                );
            }
            CaptureSource::Workspace(workspace, output) => {
                manager.capture_workspace(
                    workspace,
                    output,
                    zcosmic_screencopy_manager_v1::CursorMode::Hidden,
                    &qh,
                    udata,
                );
            }
        }
    }
}

struct SessionData {
    session_data: ScreencopySessionData,
    capture: Arc<Capture>,
}

impl ScreencopySessionDataExt for SessionData {
    fn screencopy_session_data(&self) -> &ScreencopySessionData {
        &self.session_data
    }
}

pub struct AppData {
    qh: QueueHandle<Self>,
    output_state: OutputState,
    registry_state: RegistryState,
    toplevel_info_state: ToplevelInfoState,
    workspace_state: WorkspaceState,
    screencopy_state: ScreencopyState,
    seat_state: SeatState,
    shm_state: ShmState,
    toplevel_manager_state: ToplevelManagerState,
    sender: mpsc::Sender<Event>,
    output_names: HashMap<ObjectId, Option<String>>,
    seats: Vec<wl_seat::WlSeat>,
    capture_filter: CaptureFilter,
    captures: RefCell<HashMap<CaptureSource, Arc<Capture>>>,
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
        }
    }

    fn invalidate_capture_filter(&mut self) {
        // XXX drain filter
        // TODO cancel captures if needed, enable capture
    }

    fn add_capture_source(&self, source: CaptureSource) {
        let capture = Arc::new(Capture::new(source.clone()));
        capture.capture(&self.screencopy_state.screencopy_manager, &self.qh);
        self.captures.borrow_mut().insert(source, capture);
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
    fn shm_state(&mut self) -> &mut ShmState {
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

// TODO any indication when we have all toplevels?
impl ToplevelInfoHandler for AppData {
    fn toplevel_info_state(&mut self) -> &mut ToplevelInfoState {
        &mut self.toplevel_info_state
    }

    fn new_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    ) {
        let info = self.toplevel_info_state.info(&toplevel).unwrap();
        self.send_event(Event::NewToplevel(toplevel.clone(), info.clone()));

        self.add_capture_source(CaptureSource::Toplevel(toplevel.clone()));
    }

    fn update_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    ) {
        // TODO
    }

    fn toplevel_closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    ) {
        self.send_event(Event::CloseToplevel(toplevel.clone()));
    }
}

impl WorkspaceHandler for AppData {
    fn workspace_state(&mut self) -> &mut WorkspaceState {
        &mut self.workspace_state
    }

    fn done(&mut self) {
        let mut workspaces = Vec::new();

        for group in self.workspace_state.workspace_groups() {
            for workspace in &group.workspaces {
                if let Some(output) = group.output.as_ref() {
                    let output_name = self.output_names.get(&output.id()).unwrap().clone();
                    workspaces.push((output_name, workspace.clone()));

                    self.add_capture_source(CaptureSource::Workspace(
                        workspace.handle.clone(),
                        output.clone(),
                    ));
                }
            }
        }

        self.send_event(Event::Workspaces(workspaces));
    }
}

impl ScreencopyHandler for AppData {
    fn screencopy_state(&mut self) -> &mut ScreencopyState {
        &mut self.screencopy_state
    }

    fn init_done(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        session: &zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
        buffer_infos: &[BufferInfo],
    ) {
        let capture = &session.data::<SessionData>().unwrap().capture;
        if capture.cancelled.load(Ordering::SeqCst) {
            session.destroy();
            return;
        }

        let buffer_info = buffer_infos
            .iter()
            .find(|x| {
                x.type_ == WEnum::Value(zcosmic_screencopy_session_v1::BufferType::WlShm)
                    && x.format == wl_shm::Format::Abgr8888.into()
            })
            .unwrap();
        let buf_len = buffer_info.stride * buffer_info.height;

        // XXX fix in compositor
        if buffer_info.width == 0 || buffer_info.height == 0 || buffer_info.stride == 0 {
            session.destroy();
            return;
        }

        let mut buffer = capture.buffer.lock().unwrap();
        // Create new buffer if none, or different format
        if !buffer
            .as_ref()
            .map_or(false, |x| &x.buffer_info == buffer_info)
        {
            *buffer = Some(Buffer::new(buffer_info.clone(), &self.shm_state, qh));
        }
        let buffer = buffer.as_ref().unwrap();

        session.attach_buffer(&buffer.buffer, None, 0); // XXX age?
        if capture.first_frame.load(Ordering::SeqCst) {
            session.commit(zcosmic_screencopy_session_v1::Options::empty());
        } else {
            session.commit(zcosmic_screencopy_session_v1::Options::OnDamage);
        }
        conn.flush().unwrap();
    }

    fn ready(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        session: &zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
    ) {
        let capture = &session.data::<SessionData>().unwrap().capture;
        if capture.cancelled.load(Ordering::SeqCst) {
            session.destroy();
            return;
        }

        let mut buffer = capture.buffer.lock().unwrap();
        let mut buffer = buffer.as_mut().unwrap();
        // XXX is this at all a performance issue?
        let image = image::Handle::from_pixels(
            buffer.buffer_info.width,
            buffer.buffer_info.height,
            buffer.pool.mmap().to_vec(),
        );
        let event = match &capture.source {
            CaptureSource::Toplevel(toplevel) => Event::ToplevelCapture(toplevel.clone(), image),
            CaptureSource::Workspace(workspace, _) => {
                Event::WorkspaceCapture(workspace.clone(), image)
            }
        };
        self.send_event(event);
        session.destroy();

        // Capture again on damage
        capture.first_frame.store(false, Ordering::SeqCst);
        capture.capture(&self.screencopy_state.screencopy_manager, &self.qh);
    }

    fn failed(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        session: &zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
        reason: WEnum<zcosmic_screencopy_session_v1::FailureReason>,
    ) {
        // TODO
        println!("Failed");
        session.destroy();
    }
}

impl ToplevelManagerHandler for AppData {
    fn toplevel_manager_state(&mut self) -> &mut ToplevelManagerState {
        &mut self.toplevel_manager_state
    }

    fn capabilities(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        capabilities: Vec<
            WEnum<zcosmic_toplevel_manager_v1::ZcosmicToplelevelManagementCapabilitiesV1>,
        >,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for AppData {
    fn event(
        _app_data: &mut Self,
        buffer: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_buffer::Event::Release => {}
            _ => unreachable!(),
        }
    }
}

fn start() -> mpsc::Receiver<Event> {
    let (sender, receiver) = mpsc::channel(20);

    // TODO share connection? Can't use same `WlOutput` with seperate connection
    let conn = Connection::connect_to_env().unwrap();
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    let registry_state = RegistryState::new(&globals);
    let mut app_data = AppData {
        qh: qh.clone(),
        output_state: OutputState::new(&globals, &qh),
        workspace_state: WorkspaceState::new(&registry_state, &qh), // Create before toplevel info state
        toplevel_info_state: ToplevelInfoState::new(&registry_state, &qh),
        toplevel_manager_state: ToplevelManagerState::new(&registry_state, &qh),
        screencopy_state: ScreencopyState::new(&globals, &qh),
        registry_state,
        seat_state: SeatState::new(&globals, &qh),
        shm_state: ShmState::bind(&globals, &qh).unwrap(),
        sender,
        output_names: HashMap::new(),
        seats: Vec::new(),
        capture_filter: CaptureFilter::default(),
        captures: RefCell::new(HashMap::new()),
    };

    app_data.send_event(Event::Connection(conn));
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
        WaylandSource::new(event_queue)
            .unwrap()
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
cctk::delegate_toplevel_info!(AppData);
cctk::delegate_toplevel_manager!(AppData);
cctk::delegate_workspace!(AppData);
cctk::delegate_screencopy!(AppData, session: [SessionData]);
