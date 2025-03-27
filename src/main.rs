// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

#![allow(clippy::single_match)]

use cctk::{
    cosmic_protocols::workspace::v2::client::zcosmic_workspace_handle_v2,
    sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer},
    wayland_client::{protocol::wl_output, Connection, Proxy},
    wayland_protocols::ext::workspace::v1::client::ext_workspace_handle_v1,
};
use clap::Parser;
use cosmic::{
    app::{Application, CosmicFlags},
    cctk, dbus_activation,
    iced::{
        self,
        event::wayland::{Event as WaylandEvent, LayerEvent, OutputEvent},
        keyboard::key::{Key, Named},
        mouse::ScrollDelta,
        Size, Subscription, Task,
    },
    iced_core::window::Id as SurfaceId,
    iced_runtime::platform_specific::wayland::layer_surface::{
        IcedOutput, SctkLayerSurfaceSettings,
    },
    iced_winit::platform_specific::wayland::commands::layer_surface::{
        destroy_layer_surface, get_layer_surface,
    },
};
use cosmic_comp_config::CosmicCompConfig;
use cosmic_config::{cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};
use i18n_embed::DesktopLanguageRequester;
use std::{
    collections::{HashMap, HashSet},
    mem,
    path::PathBuf,
    str,
    time::{Duration, Instant},
};

mod desktop_info;
#[macro_use]
mod localize;
mod backend;
mod view;
use backend::{ExtForeignToplevelHandleV1, ExtWorkspaceHandleV1, ToplevelInfo};
mod dnd;
mod utils;
mod widgets;
use dnd::{DragSurface, DragToplevel, DragWorkspace, DropTarget};

#[derive(Clone, Debug, Default, PartialEq, CosmicConfigEntry)]
struct CosmicWorkspacesConfig {
    show_workspace_number: bool,
    show_workspace_name: bool,
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Args {}

#[derive(Default, Debug, Clone)]
pub struct WorkspaceCommands;

#[allow(clippy::to_string_trait_impl)]
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

enum ScrollDirection {
    Next,
    Prev,
}

#[derive(Clone, Debug)]
enum Msg {
    WaylandEvent(WaylandEvent),
    Wayland(backend::Event),
    Close,
    ActivateWorkspace(ExtWorkspaceHandleV1),
    #[allow(dead_code)]
    CloseWorkspace(ExtWorkspaceHandleV1),
    ActivateToplevel(ExtForeignToplevelHandleV1),
    CloseToplevel(ExtForeignToplevelHandleV1),
    StartDrag(DragSurface),
    DndEnter(DropTarget, f64, f64, Vec<String>),
    DndLeave(DropTarget),
    DndToplevelDrop(DragToplevel),
    #[allow(dead_code)]
    DndWorkspaceDrag,
    #[allow(dead_code)]
    DndWorkspaceDrop(DragWorkspace),
    SourceFinished,
    #[allow(dead_code)]
    NewWorkspace,
    CompConfig(Box<CosmicCompConfig>),
    Config(CosmicWorkspacesConfig),
    BgConfig(cosmic_bg_config::state::State),
    UpdateToplevelIcon(String, Option<PathBuf>),
    OnScroll(wl_output::WlOutput, ScrollDelta),
    TogglePinned(ExtWorkspaceHandleV1),
    Ignore,
}

#[derive(Clone, Debug)]
struct Workspace {
    name: String,
    // img_for_output: HashMap<wl_output::WlOutput, backend::CaptureImage>,
    img: Option<backend::CaptureImage>,
    handle: ExtWorkspaceHandleV1,
    outputs: HashSet<wl_output::WlOutput>,
    coordinates: Vec<u32>,
    is_active: bool,
    is_pinned: bool,
}

#[derive(Clone, Debug)]
struct Toplevel {
    handle: ExtForeignToplevelHandleV1,
    info: ToplevelInfo,
    img: Option<backend::CaptureImage>,
    icon: Option<PathBuf>,
}

#[derive(Clone)]
struct Output {
    handle: wl_output::WlOutput,
    name: String,
    width: i32,
    height: i32,
}

#[derive(Debug)]
struct LayerSurface {
    output: wl_output::WlOutput,
    // for transitions, would need windows in more than one workspace? But don't capture all of
    // them all the time every frame.
}

#[derive(Default)]
struct Conf {
    workspace_config: cosmic_comp_config::workspace::WorkspaceConfig,
    config: CosmicWorkspacesConfig,
    bg: cosmic_bg_config::state::State,
}

#[derive(Default)]
struct App {
    layer_surfaces: HashMap<SurfaceId, LayerSurface>,
    outputs: Vec<Output>,
    workspaces: Vec<Workspace>,
    toplevels: Vec<Toplevel>,
    conn: Option<Connection>,
    visible: bool,
    wayland_cmd_sender: Option<calloop::channel::Sender<backend::Cmd>>,
    drag_surface: Option<(DragSurface, Size)>,
    conf: Conf,
    core: cosmic::app::Core,
    drop_target: Option<DropTarget>,
    scroll: Option<(f32, Instant)>,
}

impl App {
    fn workspace_for_handle(&self, handle: &ExtWorkspaceHandleV1) -> Option<&Workspace> {
        self.workspaces.iter().find(|i| &i.handle == handle)
    }

    fn workspace_for_handle_mut(
        &mut self,
        handle: &ExtWorkspaceHandleV1,
    ) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|i| &i.handle == handle)
    }

    // TODO iterate in order based on `coordinates`
    fn workspaces_for_output<'a>(
        &'a self,
        output: &'a wl_output::WlOutput,
    ) -> impl Iterator<Item = &'a Workspace> + 'a {
        self.workspaces
            .iter()
            .filter(|w| w.outputs.contains(output))
    }

    fn toplevel_for_handle_mut(
        &mut self,
        handle: &ExtForeignToplevelHandleV1,
    ) -> Option<&mut Toplevel> {
        self.toplevels.iter_mut().find(|i| &i.handle == handle)
    }

    fn create_surface(&mut self, output: wl_output::WlOutput) -> Task<cosmic::Action<Msg>> {
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

    fn destroy_surface(&mut self, output: &wl_output::WlOutput) -> Task<cosmic::Action<Msg>> {
        if let Some((id, _)) = self
            .layer_surfaces
            .iter()
            .find(|(_id, surface)| &surface.output == output)
        {
            let id = *id;
            destroy_layer_surface(id)
        } else {
            Task::none()
        }
    }

    fn toggle(&mut self) -> Task<cosmic::Action<Msg>> {
        if self.visible {
            self.hide()
        } else {
            self.show()
        }
    }

    fn show(&mut self) -> Task<cosmic::Action<Msg>> {
        if !self.visible {
            self.visible = true;
            let outputs = self.outputs.clone();
            let cmd = Task::batch(
                outputs
                    .into_iter()
                    .map(|output| self.create_surface(output.handle))
                    .collect::<Vec<_>>(),
            );
            self.update_capture_filter();

            cmd
        } else {
            Task::none()
        }
    }

    // Close all shell surfaces
    fn hide(&mut self) -> Task<cosmic::Action<Msg>> {
        self.visible = false;
        self.update_capture_filter();
        self.drag_surface = None;
        Task::batch(
            self.layer_surfaces
                .keys()
                .copied()
                .map(destroy_layer_surface),
        )
    }

    fn send_wayland_cmd(&self, cmd: backend::Cmd) {
        if let Some(sender) = self.wayland_cmd_sender.as_ref() {
            sender.send(cmd).unwrap();
        }
    }

    fn update_capture_filter(&self) {
        let mut capture_filter = backend::CaptureFilter::default();
        if self.visible {
            capture_filter.workspaces_on_outputs =
                self.outputs.iter().map(|x| x.handle.clone()).collect();
            capture_filter.toplevels_on_workspaces = self
                .workspaces
                .iter()
                .filter(|x| x.is_active)
                .map(|x| x.handle.clone())
                .collect();
        }
        self.send_wayland_cmd(backend::Cmd::CaptureFilter(capture_filter));
    }
}

impl Application for App {
    type Message = Msg;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = Args;
    const APP_ID: &'static str = "com.system76.CosmicWorkspaces";

    fn init(core: cosmic::app::Core, _flags: Self::Flags) -> (Self, Task<cosmic::Action<Msg>>) {
        (
            Self {
                core,
                ..Default::default()
            },
            Task::none(),
        )
    }
    // TODO: show panel and dock? Drag?

    fn update(&mut self, message: Msg) -> Task<cosmic::Action<Msg>> {
        match message {
            Msg::SourceFinished => {
                self.drag_surface = None;
            }
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
                WaylandEvent::Layer(LayerEvent::Done, _surface, id) => {
                    if self.layer_surfaces.remove(&id).is_none() {
                        log::error!("removing non-existant layer shell id {}?", id);
                    }
                }
                _ => {}
            },
            Msg::Wayland(evt) => {
                match evt {
                    backend::Event::CmdSender(sender) => {
                        self.wayland_cmd_sender = Some(sender);
                    }
                    backend::Event::Workspaces(mut workspaces) => {
                        workspaces.sort_by(|(_, w1), (_, w2)| w1.coordinates.cmp(&w2.coordinates));
                        let old_workspaces = mem::take(&mut self.workspaces);
                        self.workspaces = Vec::new();
                        for (outputs, workspace) in workspaces {
                            let is_active = workspace
                                .state
                                .contains(ext_workspace_handle_v1::State::Active);
                            let is_pinned = workspace
                                .cosmic_state
                                .contains(zcosmic_workspace_handle_v2::State::Pinned);

                            // XXX efficiency
                            #[allow(clippy::mutable_key_type)]
                            let img = old_workspaces
                                .iter()
                                .find(|i| i.handle == workspace.handle)
                                .map(|i| i.img.clone())
                                .unwrap_or_default();

                            self.workspaces.push(Workspace {
                                name: workspace.name,
                                handle: workspace.handle,
                                outputs,
                                coordinates: workspace.coordinates.clone(),
                                img,
                                is_active,
                                is_pinned,
                            });
                        }
                        self.update_capture_filter();
                    }
                    backend::Event::NewToplevel(handle, info) => {
                        log::debug!("New toplevel: {info:?}");
                        let app_id = info.app_id.clone();
                        let icon_task = iced::Task::perform(
                            desktop_info::icon_for_app_id(app_id.clone()),
                            move |path| Msg::UpdateToplevelIcon(app_id.clone(), path),
                        )
                        .map(cosmic::Action::App);
                        self.toplevels.push(Toplevel {
                            icon: None,
                            handle,
                            info,
                            img: None,
                        });
                        // Close workspaces view if a window spawns while open
                        #[cfg(not(feature = "mock-backend"))]
                        if self.visible {
                            return Task::batch([icon_task, self.hide()]);
                        }
                        return icon_task;
                    }
                    backend::Event::UpdateToplevel(handle, info) => {
                        if let Some(toplevel) =
                            self.toplevels.iter_mut().find(|x| x.handle == handle)
                        {
                            let mut task = Task::none();
                            if toplevel.info.app_id != info.app_id {
                                let app_id = info.app_id.clone();
                                task = iced::Task::perform(
                                    desktop_info::icon_for_app_id(app_id.clone()),
                                    move |path| Msg::UpdateToplevelIcon(app_id.clone(), path),
                                )
                                .map(cosmic::Action::App);
                            }
                            toplevel.info = info;
                            return task;
                        }
                    }
                    backend::Event::CloseToplevel(handle) => {
                        if let Some(idx) = self.toplevels.iter().position(|x| x.handle == handle) {
                            self.toplevels.remove(idx);
                        }
                    }
                    backend::Event::WorkspaceCapture(handle, image) => {
                        //println!("Workspace capture");
                        if let Some(workspace) = self.workspace_for_handle_mut(&handle) {
                            workspace.img = Some(image);
                        }
                    }
                    backend::Event::ToplevelCapture(handle, image) => {
                        if let Some(toplevel) = self.toplevel_for_handle_mut(&handle) {
                            // println!("Got toplevel image!");
                            toplevel.img = Some(image);
                        }
                    }
                }
            }
            Msg::Close => {
                return self.hide();
            }
            Msg::ActivateWorkspace(workspace_handle) => {
                if let Some(workspace) = self.workspace_for_handle(&workspace_handle) {
                    if workspace.is_active {
                        return self.hide();
                    }
                }
                self.send_wayland_cmd(backend::Cmd::ActivateWorkspace(workspace_handle));
            }
            Msg::ActivateToplevel(toplevel_handle) => {
                self.send_wayland_cmd(backend::Cmd::ActivateToplevel(toplevel_handle));
                return self.hide();
            }
            Msg::CloseWorkspace(_workspace_handle) => {
                // XXX close specific workspace
                /*
                if let WorkspaceAmount::Static(n) = &mut self.conf.workspace_config.workspace_amount
                {
                    if *n != 1 {
                        *n -= 1;
                        self.conf
                            .cosmic_comp_config
                            .set("workspaces", &self.conf.workspace_config);
                    }
                }
                */
            }
            Msg::CloseToplevel(toplevel_handle) => {
                // TODO confirmation?
                self.send_wayland_cmd(backend::Cmd::CloseToplevel(toplevel_handle));
            }
            Msg::StartDrag(drag_surface) => {
                self.drag_surface = Some((drag_surface, Default::default()));
            }
            Msg::DndEnter(drop_target, _x, _y, _mimes) => {
                self.drop_target = Some(drop_target);
            }
            Msg::DndLeave(drop_target) => {
                // Currently in iced-sctk, a `DndOfferEvent::Motion` may cause a leave event after
                // an enter event, based on which widget handles it first. So we need a test here.
                if self.drop_target == Some(drop_target) {
                    self.drop_target = None;
                }
            }
            Msg::DndToplevelDrop(_toplevel) => {
                if let Some((DragSurface::Toplevel(handle), _)) = &self.drag_surface {
                    match self.drop_target.take() {
                        Some(
                            DropTarget::WorkspaceSidebarEntry(workspace, output)
                            | DropTarget::OutputToplevels(workspace, output),
                        ) => {
                            self.send_wayland_cmd(backend::Cmd::MoveToplevelToWorkspace(
                                handle.clone(),
                                workspace,
                                output,
                            ));
                        }
                        Some(DropTarget::WorkspacesBar(_)) | None => {}
                    }
                }
            }
            Msg::NewWorkspace => {
                /*
                if let WorkspaceAmount::Static(n) = &mut self.conf.workspace_config.workspace_amount
                {
                    *n += 1;
                    self.conf
                        .cosmic_comp_config
                        .set("workspaces", &self.conf.workspace_config);
                }
                */
            }
            Msg::Config(c) => {
                self.conf.config = c;
            }
            Msg::CompConfig(c) => {
                self.conf.workspace_config = c.workspaces;
            }
            Msg::BgConfig(c) => {
                self.conf.bg = c;
            }
            Msg::UpdateToplevelIcon(app_id, path) => {
                for toplevel in self.toplevels.iter_mut() {
                    if toplevel.info.app_id == app_id {
                        toplevel.icon = path.clone();
                    }
                }
            }
            Msg::OnScroll(output, delta) => {
                // Accumulate delta with a timer
                // TODO: Should x scroll be handled too?
                // Best time/pixel count?
                let direction = match delta {
                    ScrollDelta::Pixels { x: _, mut y } => {
                        y = -y;
                        let previous_scroll = if let Some((scroll, last_scroll_time)) = self.scroll
                        {
                            if last_scroll_time.elapsed() > Duration::from_millis(100) {
                                0.
                            } else {
                                scroll
                            }
                        } else {
                            0.
                        };

                        let scroll = previous_scroll + y;
                        if scroll <= -4. {
                            self.scroll = None;
                            ScrollDirection::Prev
                        } else if scroll >= 4. {
                            self.scroll = None;
                            ScrollDirection::Next
                        } else {
                            // If scroll has y element, accumulate scroll
                            self.scroll = if y != 0. {
                                Some((scroll, Instant::now()))
                            } else {
                                None
                            };
                            return Task::none();
                        }
                    }
                    ScrollDelta::Lines { x: _, mut y } => {
                        y = -y;
                        self.scroll = None;
                        if y < 0. {
                            ScrollDirection::Prev
                        } else if y > 0. {
                            ScrollDirection::Next
                        } else {
                            return Task::none();
                        }
                    }
                };

                // TODO assumes only one active workspace per output
                let workspaces = self.workspaces_for_output(&output).collect::<Vec<_>>();
                if let Some(workspace_idx) = workspaces.iter().position(|i| i.is_active) {
                    let workspace = match direction {
                        // Next workspace on output
                        ScrollDirection::Next => workspaces[workspace_idx + 1..].iter().next(),
                        // Previous workspace on output
                        ScrollDirection::Prev => workspaces[..workspace_idx].iter().last(),
                    };
                    if let Some(workspace) = workspace {
                        self.send_wayland_cmd(backend::Cmd::ActivateWorkspace(
                            workspace.handle.clone(),
                        ));
                    }
                }
            }
            Msg::DndWorkspaceDrag => {}
            Msg::DndWorkspaceDrop(_workspace) => {
                if let Some((DragSurface::Workspace(handle), _)) = &self.drag_surface {
                    match self.drop_target.take() {
                        Some(DropTarget::WorkspaceSidebarEntry(other_handle, _output)) => {
                            let workspace = self.workspaces.iter().find(|i| i.handle == *handle);
                            let other_workspace =
                                self.workspaces.iter().find(|i| i.handle == other_handle);
                            if let (Some(workspace), Some(other_workspace)) =
                                (workspace, other_workspace)
                            {
                                self.send_wayland_cmd(
                                    if workspace.outputs == other_workspace.outputs
                                        && workspace.coordinates[0] + 1
                                            == other_workspace.coordinates[0]
                                    {
                                        backend::Cmd::MoveWorkspaceAfter(
                                            handle.clone(),
                                            other_handle,
                                        )
                                    } else {
                                        backend::Cmd::MoveWorkspaceBefore(
                                            handle.clone(),
                                            other_handle,
                                        )
                                    },
                                );
                            }
                        }
                        Some(DropTarget::OutputToplevels(_, _) | DropTarget::WorkspacesBar(_))
                        | None => {}
                    }
                }
            }
            Msg::TogglePinned(workspace_handle) => {
                if let Some(workspace) = self
                    .workspaces
                    .iter()
                    .find(|w| w.handle == workspace_handle)
                {
                    self.send_wayland_cmd(backend::Cmd::SetWorkspacePinned(
                        workspace_handle,
                        !workspace.is_pinned,
                    ));
                }
            }
            Msg::Ignore => {}
        }

        Task::none()
    }
    fn dbus_activation(&mut self, msg: dbus_activation::Message) -> Task<cosmic::Action<Msg>> {
        if let dbus_activation::Details::Activate = msg.msg {
            self.toggle()
        } else {
            Task::none()
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        let events = iced::event::listen_with(|evt, _, _| {
            if let iced::Event::PlatformSpecific(iced::event::PlatformSpecific::Wayland(evt)) = evt
            {
                if !matches!(evt, WaylandEvent::RequestResize) {
                    Some(Msg::WaylandEvent(evt))
                } else {
                    None
                }
            } else if let iced::Event::Keyboard(iced::keyboard::Event::KeyReleased {
                key: Key::Named(Named::Escape),
                modifiers: _,
                location: _,
                modified_key: _,
                physical_key: _,
            }) = evt
            {
                Some(Msg::Close)
            } else {
                None
            }
        });
        let config_subscription = cosmic_config::config_subscription::<_, CosmicWorkspacesConfig>(
            "config-sub",
            "com.system76.CosmicWorkspaces".into(),
            1,
        )
        .map(|update| {
            if !update.errors.is_empty() {
                log::error!("Failed to load workspaces config: {:?}", update.errors);
            }
            Msg::Config(update.config)
        });
        let comp_config_subscription = cosmic_config::config_subscription::<_, CosmicCompConfig>(
            "comp-config-sub",
            "com.system76.CosmicComp".into(),
            1,
        )
        .map(|update| {
            if !update.errors.is_empty() {
                log::error!("Failed to load compositor config: {:?}", update.errors);
            }
            Msg::CompConfig(Box::new(update.config))
        });
        let bg_subscription =
            cosmic_config::config_state_subscription::<_, cosmic_bg_config::state::State>(
                "bg-sub",
                cosmic_bg_config::NAME.into(),
                cosmic_bg_config::state::State::version(),
            )
            .map(|update| {
                if !update.errors.is_empty() {
                    log::error!("Failed to load bg config: {:?}", update.errors);
                }
                Msg::BgConfig(update.config)
            });

        let mut subscriptions = vec![
            events,
            config_subscription,
            comp_config_subscription,
            bg_subscription,
        ];
        if let Some(conn) = self.conn.clone() {
            subscriptions.push(backend::subscription(conn).map(Msg::Wayland));
        }
        iced::Subscription::batch(subscriptions)
    }

    fn view(&self) -> cosmic::prelude::Element<Self::Message> {
        unreachable!()
    }

    fn view_window(&self, id: iced::window::Id) -> cosmic::prelude::Element<Self::Message> {
        if let Some(surface) = self.layer_surfaces.get(&id) {
            return view::layer_surface(self, surface);
        }
        log::error!("non-existant layer shell id {}?", id);
        cosmic::widget::text("workspaces").into()
    }

    fn on_close_requested(&self, _id: SurfaceId) -> Option<Msg> {
        None
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }
}

fn init_localizer() {
    let localizer = crate::localize::localizer();
    let requested_languages = DesktopLanguageRequester::requested_languages();

    if let Err(why) = localizer.select(&requested_languages) {
        log::error!("error while loading fluent localizations: {}", why);
    }
}

pub fn main() -> iced::Result {
    env_logger::init();
    init_localizer();

    cosmic::app::run_single_instance::<App>(
        cosmic::app::Settings::default()
            .no_main_window(true)
            .exit_on_close(false),
        Args::parse(),
    )
}
