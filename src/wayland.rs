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
        workspace::v1::client::zcosmic_workspace_handle_v1,
    },
    screencopy::{BufferInfo, ScreencopyHandler, ScreencopyState},
    sctk::{
        self,
        output::{OutputHandler, OutputState},
        registry::{ProvidesRegistryState, RegistryState},
        shm::{raw::RawPool, ShmHandler, ShmState},
    },
    toplevel_info::{ToplevelInfo, ToplevelInfoHandler, ToplevelInfoState},
    wayland_client::{
        backend::ObjectId,
        globals::registry_queue_init,
        protocol::{wl_buffer, wl_output, wl_shm},
        Connection, Dispatch, Proxy, QueueHandle, WEnum,
    },
    workspace::{WorkspaceHandler, WorkspaceState},
};
use futures_channel::mpsc;
use iced::{
    futures::{executor::block_on, FutureExt, SinkExt},
    widget::image,
};
use std::{collections::HashMap, thread};

// TODO define subscription for a particular output/workspace/toplevel (but we want to rate limit?)

#[derive(Clone, Debug)]
pub enum Event {
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
    ToplevelCapture(
        zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
        image::Handle,
    ),
}

pub fn subscription() -> iced::Subscription<Event> {
    iced::subscription::run("wayland-sub", async { start() }.flatten_stream())
}

enum CaptureSource {
    Toplevel(zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1),
    Workspace(zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1),
}

struct Frame {
    buffer: Option<(RawPool, wl_buffer::WlBuffer, BufferInfo)>,
    source: CaptureSource,
    first_frame: bool,
}

struct AppData {
    qh: QueueHandle<Self>,
    output_state: OutputState,
    registry_state: RegistryState,
    toplevel_info_state: ToplevelInfoState,
    workspace_state: WorkspaceState,
    screencopy_state: ScreencopyState,
    shm_state: ShmState,
    sender: mpsc::Sender<Event>,
    frames: HashMap<ObjectId, Frame>,
    output_names: HashMap<ObjectId, Option<String>>,
}

impl AppData {
    fn send_event(&mut self, event: Event) {
        let _ = block_on(self.sender.send(event));
    }
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    sctk::registry_handlers!(OutputState,);
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

        let frame = self.screencopy_state.screencopy_manager.capture_toplevel(
            toplevel,
            zcosmic_screencopy_manager_v1::CursorMode::Hidden,
            &self.qh,
            Default::default(), // TODO
        );
        // XXX first_frame
        self.frames.insert(
            frame.id(),
            Frame {
                buffer: None,
                source: CaptureSource::Toplevel(toplevel.clone()),
                first_frame: true,
            },
        );
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
        // TODO
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

                    let frame = self.screencopy_state.screencopy_manager.capture_workspace(
                        &workspace.handle,
                        output,
                        zcosmic_screencopy_manager_v1::CursorMode::Hidden,
                        &self.qh,
                        Default::default(), // TODO
                    );
                    // XXX first_frame
                    self.frames.insert(
                        frame.id(),
                        Frame {
                            buffer: None,
                            source: CaptureSource::Workspace(workspace.handle.clone()),
                            first_frame: true,
                        },
                    );
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
        let buffer_info = buffer_infos
            .iter()
            .find(|x| {
                x.type_ == WEnum::Value(zcosmic_screencopy_session_v1::BufferType::WlShm)
                    && x.format == wl_shm::Format::Abgr8888.into()
            })
            .unwrap();
        let buf_len = buffer_info.stride * buffer_info.height;

        let mut pool = RawPool::new(buf_len as usize, &self.shm_state).unwrap();
        let buffer = pool.create_buffer(
            0,
            buffer_info.width as i32,
            buffer_info.height as i32,
            buffer_info.stride as i32,
            wl_shm::Format::Abgr8888,
            (),
            qh,
        );

        let mut frame = self.frames.get_mut(&session.id()).unwrap();

        session.attach_buffer(&buffer, None, 0); // XXX age?
        if frame.first_frame {
            session.commit(zcosmic_screencopy_session_v1::Options::empty());
        } else {
            session.commit(zcosmic_screencopy_session_v1::Options::OnDamage);
        }
        conn.flush().unwrap();

        frame.buffer = Some((pool, buffer, buffer_info.clone()));
    }

    fn ready(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        session: &zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
    ) {
        let frame = self.frames.get_mut(&session.id()).unwrap();
        let (mut pool, buffer, buffer_info) = frame.buffer.take().unwrap();
        let image =
            image::Handle::from_pixels(buffer_info.width, buffer_info.height, pool.mmap().to_vec());
        let event = match &frame.source {
            CaptureSource::Toplevel(toplevel) => Event::ToplevelCapture(toplevel.clone(), image),
            CaptureSource::Workspace(workspace) => {
                Event::WorkspaceCapture(workspace.clone(), image)
            }
        };
        self.send_event(event);
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
        screencopy_state: ScreencopyState::new(&globals, &qh),
        registry_state,
        shm_state: ShmState::bind(&globals, &qh).unwrap(),
        sender,
        frames: HashMap::new(),
        output_names: HashMap::new(),
    };

    thread::spawn(move || loop {
        event_queue.blocking_dispatch(&mut app_data).unwrap();
    });

    receiver
}

sctk::delegate_output!(AppData);
sctk::delegate_registry!(AppData);
sctk::delegate_shm!(AppData);
cctk::delegate_toplevel_info!(AppData);
cctk::delegate_workspace!(AppData);
cctk::delegate_screencopy!(AppData);
