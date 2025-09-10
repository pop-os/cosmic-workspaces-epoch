// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    cctk::{
        cosmic_protocols::{
            toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
            toplevel_management::v1::client::zcosmic_toplevel_manager_v1,
            workspace::v2::client::zcosmic_workspace_handle_v2,
        },
        wayland_client::{
            Connection, WEnum,
            protocol::{wl_output, wl_shm},
        },
        wayland_protocols::ext::workspace::v1::client::ext_workspace_handle_v1,
    },
    iced::{
        self,
        futures::{FutureExt, SinkExt, executor::block_on},
    },
    iced_winit::platform_specific::wayland::subsurface_widget::{Shmbuf, SubsurfaceBuffer},
};

use futures_channel::mpsc;
use std::{
    collections::HashSet,
    fs,
    io::{self, Write},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    thread,
};

use super::{CaptureImage, Cmd, Event};
use crate::utils;

#[derive(Eq, PartialEq, Clone, Debug, Hash)]
pub struct MockObjectId(u32);

impl MockObjectId {
    pub fn protocol_id(&self) -> u32 {
        self.0
    }
}

fn create_solid_capture_image(r: u8, g: u8, b: u8) -> CaptureImage {
    let file = fs::File::from(utils::create_memfile().unwrap());
    let mut file = io::BufWriter::new(file);

    for i in 0..512 * 512 {
        file.write(&[b, g, r, 255]).unwrap();
    }

    CaptureImage {
        width: 512,
        height: 512,
        wl_buffer: SubsurfaceBuffer::new(Arc::new(
            Shmbuf {
                fd: file.into_inner().unwrap().into(),
                offset: 0,
                width: 512,
                height: 512,
                stride: 512 * 4,
                format: wl_shm::Format::Argb8888,
            }
            .into(),
        ))
        .0,
        transform: wl_output::Transform::Normal,
        #[cfg(feature = "no-subsurfaces")]
        image: cosmic::widget::image::Handle::from_rgba(512, 512, [r, g, b, 255].repeat(512 * 512)),
    }
}

impl MockObjectId {
    fn new() -> Self {
        static NEXT_MOCK_ID: AtomicU32 = AtomicU32::new(0);
        Self(NEXT_MOCK_ID.fetch_add(1, Ordering::SeqCst))
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Hash)]
pub struct ExtWorkspaceHandleV1(MockObjectId);

impl ExtWorkspaceHandleV1 {
    pub fn id(&self) -> MockObjectId {
        self.0.clone()
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Hash)]
pub struct ExtForeignToplevelHandleV1(MockObjectId);

#[derive(Clone, Debug, Default)]
pub struct ToplevelInfo {
    pub title: String,
    pub app_id: String,
    pub state: HashSet<zcosmic_toplevel_handle_v1::State>,
    pub output: HashSet<wl_output::WlOutput>,
    pub workspace: HashSet<ExtWorkspaceHandleV1>,
}

#[derive(Clone, Debug)]
pub struct Workspace {
    pub handle: ExtWorkspaceHandleV1,
    pub name: String,
    pub coordinates: Vec<u32>,
    pub state: ext_workspace_handle_v1::State,
    pub capabilities: ext_workspace_handle_v1::WorkspaceCapabilities,
    pub cosmic_capabilities: zcosmic_workspace_handle_v2::WorkspaceCapabilities,
    pub cosmic_state: zcosmic_workspace_handle_v2::State,
    // pub tiling: Option<WEnum<zcosmic_workspace_handle_v1::TilingState>>,
}

pub fn subscription(conn: Connection) -> iced::Subscription<Event> {
    iced::Subscription::run_with_id("wayland-mock-sub", async { start(conn) }.flatten_stream())
}

struct AppData {
    sender: mpsc::Sender<Event>,
    outputs: Vec<wl_output::WlOutput>,
    workspaces: Vec<(HashSet<wl_output::WlOutput>, Workspace)>,
}

impl AppData {
    fn send_event(&mut self, event: Event) {
        let _ = block_on(self.sender.send(event));
    }

    fn add_output(&mut self, output: &wl_output::WlOutput) {
        // Add four workspaces for each output
        let mut new_workspaces = Vec::new();
        for i in 0..=4 {
            let workspace_handle = ExtWorkspaceHandleV1(MockObjectId::new());
            let workspace = Workspace {
                handle: workspace_handle.clone(),
                name: format!("Workspace {i}"),
                coordinates: vec![i],
                state: if i == 0 {
                    ext_workspace_handle_v1::State::Active
                } else {
                    ext_workspace_handle_v1::State::empty()
                },
                capabilities: ext_workspace_handle_v1::WorkspaceCapabilities::Activate,
                cosmic_capabilities: zcosmic_workspace_handle_v2::WorkspaceCapabilities::empty(),
                cosmic_state: zcosmic_workspace_handle_v2::State::empty(),
            };
            // Add three toplevels for each workspace
            for j in 0..=3 {
                let toplevel_handle = ExtForeignToplevelHandleV1(MockObjectId::new());
                let toplevel_info = ToplevelInfo {
                    title: format!("App {}", j),
                    app_id: "com.example.app".to_string(),
                    state: if i == 0 {
                        HashSet::from([zcosmic_toplevel_handle_v1::State::Activated])
                    } else {
                        HashSet::new()
                    },
                    output: HashSet::from([output.clone()]),
                    workspace: HashSet::from([workspace_handle.clone()]),
                };
                self.send_event(Event::NewToplevel(toplevel_handle.clone(), toplevel_info));
                self.send_event(Event::ToplevelCapture(
                    toplevel_handle,
                    create_solid_capture_image(255, 0, 0),
                ));
            }
            self.workspaces
                .push((HashSet::from([output.clone()]), workspace));
            new_workspaces.push(workspace_handle);
        }
        self.send_event(Event::Workspaces(self.workspaces.clone()));
        for workspace_handle in new_workspaces {
            self.send_event(Event::WorkspaceCapture(
                workspace_handle,
                create_solid_capture_image(0, 255, 0),
            ));
        }
        self.outputs.push(output.clone());
    }

    fn handle_cmd(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::CaptureFilter(filter) => {
                for output in &filter.workspaces_on_outputs {
                    if !self.outputs.contains(output) {
                        self.add_output(output);
                    }
                }
            }
            Cmd::ActivateToplevel(toplevel_handle) => {
                println!("Activate {:?}", toplevel_handle);
            }
            Cmd::CloseToplevel(toplevel_handle) => {
                println!("Close {:?}", toplevel_handle);
            }
            Cmd::MoveToplevelToWorkspace(toplevel_handle, workspace_handle, output) => {}
            Cmd::ActivateWorkspace(workspace_handle) => {
                println!("Activate {:?}", workspace_handle);
            }
            // TODO
            Cmd::MoveWorkspaceBefore(_, _)
            | Cmd::MoveWorkspaceAfter(_, _)
            | Cmd::SetWorkspacePinned(_, _) => {}
        }
    }
}

fn start(_conn: Connection) -> mpsc::Receiver<Event> {
    let (sender, receiver) = mpsc::channel(20);
    thread::spawn(move || {
        let mut event_loop = calloop::EventLoop::try_new().unwrap();
        let (cmd_sender, cmd_channel) = calloop::channel::channel();
        event_loop
            .handle()
            .insert_source(cmd_channel, |cmd, (), app_data: &mut AppData| match cmd {
                calloop::channel::Event::Msg(cmd) => app_data.handle_cmd(cmd),
                calloop::channel::Event::Closed => {}
            })
            .unwrap();

        let mut app_data = AppData {
            sender,
            outputs: Vec::new(),
            workspaces: Vec::new(),
        };
        app_data.send_event(Event::ToplevelCapabilities(vec![
            zcosmic_toplevel_manager_v1::ZcosmicToplelevelManagementCapabilitiesV1::Close,
            zcosmic_toplevel_manager_v1::ZcosmicToplelevelManagementCapabilitiesV1::Activate,
            zcosmic_toplevel_manager_v1::ZcosmicToplelevelManagementCapabilitiesV1::MoveToWorkspace,
        ]));
        app_data.send_event(Event::CmdSender(cmd_sender));
        loop {
            event_loop.dispatch(None, &mut app_data).unwrap();
        }
    });
    receiver
}

// TODO WorkspaceCapture, ToplevelCapture, NewToplevel, Workspaces
