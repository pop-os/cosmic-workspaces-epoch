#![allow(clippy::single_match)]

use cctk::{
    cosmic_protocols::{
        toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
        toplevel_management::v1::client::zcosmic_toplevel_manager_v1,
        workspace::v1::client::{zcosmic_workspace_handle_v1, zcosmic_workspace_manager_v1},
    },
    sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer},
    toplevel_info::ToplevelInfo,
    wayland_client::{
        backend::ObjectId,
        protocol::{wl_data_device_manager::DndAction, wl_output, wl_seat},
        Connection, Proxy, WEnum,
    },
};
use clap::Parser;
use cosmic::{
    app::{Application, CosmicFlags, DbusActivationDetails, Message},
    cctk,
    iced::{
        self,
        event::wayland::{Event as WaylandEvent, OutputEvent},
        keyboard::KeyCode,
        wayland::{
            actions::data_device::{DataFromMimeType, DndIcon},
            data_device::{accept_mime_type, request_dnd_data, set_actions, start_drag},
        },
        widget, Command, Size, Subscription,
    },
    iced_runtime::{
        command::platform_specific::wayland::layer_surface::{
            IcedOutput, SctkLayerSurfaceSettings,
        },
        window::Id as SurfaceId,
    },
    iced_sctk::commands::layer_surface::{destroy_layer_surface, get_layer_surface},
};
use cosmic_config::ConfigGet;
use once_cell::sync::Lazy;
use std::{
    collections::{HashMap, HashSet},
    mem,
    str::{self, FromStr},
};

mod view;
mod wayland;

// Include `pid` in mime. Want to drag between our surfaces, but not another
// process, if we use Wayland object ids.
static WORKSPACE_MIME: Lazy<String> =
    Lazy::new(|| format!("text/x.cosmic-workspace-id-{}", std::process::id()));

static TOPLEVEL_MIME: Lazy<String> =
    Lazy::new(|| format!("text/x.cosmic-toplevel-id-{}", std::process::id()));

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Args {}

#[derive(Default, Debug, Clone)]
pub struct WorkspaceCommands;

impl ToString for WorkspaceCommands {
    fn to_string(&self) -> String {
        String::new()
    }
}

impl CosmicFlags for Args {
    type SubCommand = WorkspaceCommands;
    type Args = Vec<String>;

    fn action(&self) -> Option<&WorkspaceCommands> {
        None
    }
}

struct WlDndId {
    id: ObjectId,
    mime_type: &'static str,
}

impl DataFromMimeType for WlDndId {
    fn from_mime_type(&self, mime_type: &str) -> Option<Vec<u8>> {
        if mime_type == self.mime_type {
            Some(self.id.protocol_id().to_string().into_bytes())
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
enum Msg {
    WaylandEvent(WaylandEvent),
    Wayland(wayland::Event),
    Close,
    Closed(SurfaceId),
    ActivateWorkspace(zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1),
    CloseWorkspace(zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1),
    ActivateToplevel(zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1),
    CloseToplevel(zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1),
    StartDrag(Size, DragSurface),
    DndWorkspaceEnter(
        zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
        wl_output::WlOutput,
        DndAction,
        Vec<String>,
        (f32, f32),
    ),
    DndWorkspaceLeave,
    DndWorkspaceDrop,
    DndWorkspaceData(String, Vec<u8>),
}

#[derive(Debug)]
struct Workspace {
    name: String,
    img_for_output: HashMap<wl_output::WlOutput, wayland::CaptureImage>,
    handle: zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
    outputs: HashSet<wl_output::WlOutput>,
    is_active: bool,
}

#[derive(Debug)]
struct Toplevel {
    handle: zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    info: ToplevelInfo,
    img: Option<wayland::CaptureImage>,
}

#[derive(Clone)]
struct Output {
    handle: wl_output::WlOutput,
    name: String,
    width: i32,
    height: i32,
}

struct LayerSurface {
    output: wl_output::WlOutput,
    // for transitions, would need windows in more than one workspace? But don't capture all of
    // them all the time every frame.
}

#[derive(Clone, Debug)]
enum DragSurface {
    #[allow(dead_code)]
    Workspace {
        handle: zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
        output: wl_output::WlOutput,
    },
    Toplevel {
        handle: zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
        output: wl_output::WlOutput,
    },
}

struct Conf {
    _cosmic_comp_config: cosmic_config::Config,
    workspace_config: cosmic_comp_config::workspace::WorkspaceConfig,
}

impl Default for Conf {
    fn default() -> Self {
        let cosmic_comp_config = cosmic_config::Config::new("com.system76.CosmicComp", 1).unwrap();
        let workspace_config = cosmic_comp_config.get("workspaces").unwrap_or_else(|err| {
            eprintln!("Failed to read config 'worspaces': {}", err);
            cosmic_comp_config::workspace::WorkspaceConfig::default()
        });
        Self {
            _cosmic_comp_config: cosmic_comp_config,
            workspace_config,
        }
    }
}

#[derive(Default)]
struct App {
    layer_surfaces: HashMap<SurfaceId, LayerSurface>,
    outputs: Vec<Output>,
    workspaces: Vec<Workspace>,
    toplevels: Vec<Toplevel>,
    conn: Option<Connection>,
    workspace_manager: Option<zcosmic_workspace_manager_v1::ZcosmicWorkspaceManagerV1>,
    toplevel_manager: Option<zcosmic_toplevel_manager_v1::ZcosmicToplevelManagerV1>,
    seats: Vec<wl_seat::WlSeat>,
    visible: bool,
    wayland_cmd_sender: Option<calloop::channel::Sender<wayland::Cmd>>,
    drag_surface: Option<(SurfaceId, DragSurface, Size)>,
    conf: Conf,
    core: cosmic::app::Core,
    drop_target: Option<(
        zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
        wl_output::WlOutput,
    )>,
}

impl App {
    fn workspace_for_handle(
        &self,
        handle: &zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
    ) -> Option<&Workspace> {
        self.workspaces.iter().find(|i| &i.handle == handle)
    }

    fn workspace_for_handle_mut(
        &mut self,
        handle: &zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
    ) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|i| &i.handle == handle)
    }

    fn toplevel_for_handle_mut(
        &mut self,
        handle: &zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    ) -> Option<&mut Toplevel> {
        self.toplevels.iter_mut().find(|i| &i.handle == handle)
    }

    fn create_surface(
        &mut self,
        output: wl_output::WlOutput,
    ) -> Command<cosmic::app::Message<Msg>> {
        let id = SurfaceId::unique();
        self.layer_surfaces.insert(
            id,
            LayerSurface {
                output: output.clone(),
            },
        );
        get_layer_surface(SctkLayerSurfaceSettings {
            id,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            namespace: "cosmic-workspace-overview".into(),
            layer: Layer::Overlay,
            size: Some((None, None)),
            output: IcedOutput::Output(output),
            anchor: Anchor::all(),
            ..Default::default()
        })
    }

    fn destroy_surface(
        &mut self,
        output: &wl_output::WlOutput,
    ) -> Command<cosmic::app::Message<Msg>> {
        if let Some((id, _)) = self
            .layer_surfaces
            .iter()
            .find(|(_id, surface)| &surface.output == output)
        {
            let id = *id;
            self.layer_surfaces.remove(&id).unwrap();
            destroy_layer_surface(id)
        } else {
            Command::none()
        }
    }

    fn toggle(&mut self) -> Command<cosmic::app::Message<Msg>> {
        if self.visible {
            self.hide()
        } else {
            self.show()
        }
    }

    fn show(&mut self) -> Command<cosmic::app::Message<Msg>> {
        if !self.visible {
            self.visible = true;
            let outputs = self.outputs.clone();
            let cmd = Command::batch(
                outputs
                    .into_iter()
                    .map(|output| self.create_surface(output.handle))
                    .collect::<Vec<_>>(),
            );
            self.update_capture_filter();
            cmd
        } else {
            Command::none()
        }
    }

    // Close all shell surfaces
    fn hide(&mut self) -> Command<cosmic::app::Message<Msg>> {
        self.visible = false;
        self.update_capture_filter();
        Command::batch(
            mem::take(&mut self.layer_surfaces)
                .into_keys()
                .map(destroy_layer_surface)
                .collect::<Vec<_>>(),
        )
    }

    fn update_capture_filter(&self) {
        if let Some(sender) = self.wayland_cmd_sender.as_ref() {
            let mut capture_filter = wayland::CaptureFilter::default();
            if self.visible {
                // XXX handle on wrong connection
                capture_filter.workspaces_on_outputs =
                    self.outputs.iter().map(|x| x.handle.clone()).collect();
                capture_filter.toplevels_on_workspaces = self
                    .workspaces
                    .iter()
                    .filter(|x| x.is_active)
                    .map(|x| x.handle.clone())
                    .collect();
            }
            let _ = sender.send(wayland::Cmd::CaptureFilter(capture_filter));
        }
    }
}

impl Application for App {
    type Message = Msg;
    type Executor = iced::executor::Default;
    type Flags = Args;
    const APP_ID: &'static str = "com.system76.CosmicWorkspaces";

    fn init(
        core: cosmic::app::Core,
        _flags: Self::Flags,
    ) -> (Self, iced::Command<Message<Self::Message>>) {
        (
            Self {
                core,
                ..Default::default()
            },
            Command::none(),
        )
    }
    // TODO: show panel and dock? Drag?

    fn update(&mut self, message: Msg) -> Command<cosmic::app::Message<Msg>> {
        match message {
            Msg::WaylandEvent(evt) => match evt {
                WaylandEvent::Output(evt, output) => {
                    // TODO: Less hacky way to get connection from iced-sctk
                    if self.conn.is_none() {
                        if let Some(backend) = output.backend().upgrade() {
                            self.conn = Some(Connection::from_backend(backend));
                        }
                    }

                    match evt {
                        OutputEvent::Created(Some(info)) => {
                            if let (Some((width, height)), Some(name)) =
                                (info.logical_size, info.name)
                            {
                                self.outputs.push(Output {
                                    handle: output.clone(),
                                    name: name.clone(),
                                    width,
                                    height,
                                });
                                if self.visible {
                                    return self.create_surface(output.clone());
                                }
                            }
                        }
                        OutputEvent::Created(None) => {} // XXX?
                        OutputEvent::InfoUpdate(info) => {
                            if let Some(output) =
                                self.outputs.iter_mut().find(|x| x.handle == output)
                            {
                                if let Some((width, height)) = info.logical_size {
                                    output.width = width;
                                    output.height = height;
                                }
                                if let Some(name) = info.name {
                                    output.name = name;
                                }
                                // XXX re-create surface?
                            }
                        }
                        OutputEvent::Removed => {
                            if let Some(idx) = self.outputs.iter().position(|x| x.handle == output)
                            {
                                self.outputs.remove(idx);
                            }
                            if self.visible {
                                return self.destroy_surface(&output);
                            }
                        }
                    }
                }
                _ => {}
            },
            Msg::Wayland(evt) => {
                match evt {
                    wayland::Event::CmdSender(sender) => {
                        self.wayland_cmd_sender = Some(sender);
                    }
                    wayland::Event::ToplevelManager(manager) => {
                        self.toplevel_manager = Some(manager);
                    }
                    wayland::Event::WorkspaceManager(manager) => {
                        self.workspace_manager = Some(manager);
                    }
                    wayland::Event::Workspaces(workspaces) => {
                        let old_workspaces = mem::take(&mut self.workspaces);
                        self.workspaces = Vec::new();
                        for (outputs, workspace) in workspaces {
                            let is_active = workspace.state.contains(&WEnum::Value(
                                zcosmic_workspace_handle_v1::State::Active,
                            ));

                            // XXX efficiency
                            let img_for_output = old_workspaces
                                .iter()
                                .find(|i| i.handle == workspace.handle)
                                .map(|i| i.img_for_output.clone())
                                .unwrap_or_default();

                            self.workspaces.push(Workspace {
                                name: workspace.name,
                                handle: workspace.handle,
                                outputs,
                                img_for_output,
                                is_active,
                            });
                        }
                        self.update_capture_filter();
                    }
                    wayland::Event::NewToplevel(handle, info) => {
                        println!("New toplevel: {info:?}");
                        self.toplevels.push(Toplevel {
                            handle,
                            info,
                            img: None,
                        });
                    }
                    wayland::Event::UpdateToplevel(handle, info) => {
                        if let Some(toplevel) =
                            self.toplevels.iter_mut().find(|x| x.handle == handle)
                        {
                            toplevel.info = info;
                        }
                    }
                    wayland::Event::CloseToplevel(handle) => {
                        if let Some(idx) = self.toplevels.iter().position(|x| x.handle == handle) {
                            self.toplevels.remove(idx);
                        }
                    }
                    wayland::Event::WorkspaceCapture(handle, output_name, image) => {
                        if let Some(workspace) = self.workspace_for_handle_mut(&handle) {
                            workspace.img_for_output.insert(output_name, image);
                        }
                    }
                    wayland::Event::ToplevelCapture(handle, image) => {
                        if let Some(toplevel) = self.toplevel_for_handle_mut(&handle) {
                            //println!("Got toplevel image!");
                            toplevel.img = Some(image);
                        }
                    }
                    wayland::Event::Seats(seats) => {
                        self.seats = seats;
                    }
                }
            }
            Msg::Close => {
                return self.hide();
            }
            Msg::Closed(_) => {}
            Msg::ActivateWorkspace(workspace_handle) => {
                let workspace_manager = self.workspace_manager.as_ref().unwrap();
                workspace_handle.activate();
                workspace_manager.commit();
                let _ = self.conn.as_ref().unwrap().flush();
            }
            Msg::ActivateToplevel(toplevel_handle) => {
                if let Some(toplevel_manager) = self.toplevel_manager.as_ref() {
                    if !self.seats.is_empty() {
                        for seat in &self.seats {
                            toplevel_manager.activate(&toplevel_handle, seat);
                        }
                        let _ = self.conn.as_ref().unwrap().flush();
                        return self.hide();
                    }
                }
            }
            Msg::CloseWorkspace(_workspace_handle) => {}
            Msg::CloseToplevel(toplevel_handle) => {
                // TODO confirmation?
                if let Some(toplevel_manager) = self.toplevel_manager.as_ref() {
                    toplevel_manager.close(&toplevel_handle);
                }
            }
            Msg::StartDrag(size, drag_surface) => {
                let (wl_id, output, mime_type) = match &drag_surface {
                    DragSurface::Workspace { handle, output } => {
                        (handle.clone().id(), output, &*WORKSPACE_MIME)
                    }
                    DragSurface::Toplevel { handle, output } => {
                        (handle.clone().id(), output, &*TOPLEVEL_MIME)
                    }
                };
                let id = SurfaceId::unique();
                if let Some((parent_id, _)) = self
                    .layer_surfaces
                    .iter()
                    .find(|(_, x)| &x.output == output)
                {
                    self.drag_surface = Some((id, drag_surface, size));
                    return start_drag(
                        vec![mime_type.to_string()],
                        DndAction::Move,
                        *parent_id,
                        Some(DndIcon::Custom(id)),
                        Box::new(WlDndId {
                            id: wl_id,
                            mime_type,
                        }),
                    );
                }
            }
            Msg::DndWorkspaceEnter(handle, output, action, mimes, (_x, _y)) => {
                self.drop_target = Some((handle, output));
                // XXX
                // if mimes.iter().any(|x| x == WORKSPACE_MIME) && action == DndAction::Move {
                if mimes.iter().any(|x| x == &*TOPLEVEL_MIME) {
                    return Command::batch(vec![
                        set_actions(DndAction::Move, DndAction::Move),
                        accept_mime_type(Some(TOPLEVEL_MIME.to_string())),
                    ]);
                }
            }
            Msg::DndWorkspaceLeave => {
                self.drop_target = None;
                return accept_mime_type(None);
            }
            Msg::DndWorkspaceDrop => {
                return request_dnd_data(TOPLEVEL_MIME.to_string());
            }
            Msg::DndWorkspaceData(mime_type, data) => {
                if mime_type == *TOPLEVEL_MIME {
                    // XXX getting empty data?
                    let _protocol_id = str::from_utf8(&data)
                        .ok()
                        .and_then(|s| u32::from_str(s).ok());
                    if let Some((_, DragSurface::Toplevel { handle, .. }, _)) = &self.drag_surface {
                        if let Some(toplevel) = self.toplevels.iter().find(|t| &t.handle == handle)
                        {
                            if let Some(drop_target) = &self.drop_target {
                                dbg!(drop_target, toplevel);
                            }
                        }
                    }
                }
            }
        }

        Command::none()
    }
    fn dbus_activation(
        &mut self,
        msg: cosmic::app::DbusActivationMessage,
    ) -> iced::Command<cosmic::app::Message<Self::Message>> {
        if let DbusActivationDetails::Activate = msg.msg {
            self.toggle()
        } else {
            Command::none()
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        let events = iced::event::listen_with(|evt, _| {
            if let iced::Event::PlatformSpecific(iced::event::PlatformSpecific::Wayland(evt)) = evt
            {
                Some(Msg::WaylandEvent(evt))
            } else if let iced::Event::Keyboard(iced::keyboard::Event::KeyReleased {
                key_code: KeyCode::Escape,
                modifiers: _,
            }) = evt
            {
                Some(Msg::Close)
            } else {
                None
            }
        });
        let mut subscriptions = vec![events];
        if let Some(conn) = self.conn.clone() {
            subscriptions.push(wayland::subscription(conn).map(Msg::Wayland));
        }
        iced::Subscription::batch(subscriptions)
    }

    fn view(&self) -> cosmic::prelude::Element<Self::Message> {
        unreachable!()
    }

    fn view_window(&self, id: iced::window::Id) -> cosmic::prelude::Element<Self::Message> {
        use iced::widget::*;
        if let Some(surface) = self.layer_surfaces.get(&id) {
            return view::layer_surface(self, surface);
        }
        if let Some((drag_id, drag_surface, size)) = &self.drag_surface {
            if drag_id == &id {
                match drag_surface {
                    DragSurface::Workspace { handle, output } => {
                        if let Some(workspace) =
                            self.workspaces.iter().find(|x| &x.handle == handle)
                        {
                            let item = view::workspace_item(workspace, output);
                            return widget::container(item)
                                .height(iced::Length::Fixed(size.height))
                                .width(iced::Length::Fixed(size.width))
                                .into();
                        }
                    }
                    DragSurface::Toplevel { handle, .. } => {
                        if let Some(toplevel) = self.toplevels.iter().find(|x| &x.handle == handle)
                        {
                            let item = view::toplevel_preview(toplevel);
                            return widget::container(item)
                                .height(iced::Length::Fixed(size.height))
                                .width(iced::Length::Fixed(size.width))
                                .into();
                        }
                    }
                }
            }
        }
        println!("NO VIEW");
        text("workspaces").into()
    }

    fn on_close_requested(&self, id: SurfaceId) -> Option<Msg> {
        Some(Msg::Closed(id))
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn style(
        &self,
    ) -> Option<<cosmic::Theme as cosmic::iced_style::application::StyleSheet>::Style> {
        Some(cosmic::theme::style::iced::Application::default())
    }
}

pub fn main() -> iced::Result {
    env_logger::init();

    cosmic::app::run_single_instance::<App>(
        cosmic::app::Settings::default()
            .antialiasing(true)
            .no_main_window(true)
            .exit_on_close(false),
        Args::parse(),
    )
}
