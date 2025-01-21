// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

#![allow(clippy::single_match)]

use cctk::{
    cosmic_protocols::workspace::v1::client::zcosmic_workspace_handle_v1,
    sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer},
    wayland_client::{protocol::wl_output, Connection, Proxy, WEnum},
};
use clap::Parser;
use cosmic::{
    app::{Application, CosmicFlags, DbusActivationDetails, Message},
    cctk,
    iced::{
        self,
        clipboard::mime::AsMimeTypes,
        event::wayland::{Event as WaylandEvent, LayerEvent, OutputEvent},
        keyboard::key::{Key, Named},
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
    borrow::Cow,
    collections::{HashMap, HashSet},
    mem,
    path::PathBuf,
    str,
    sync::LazyLock,
};

mod desktop_info;
#[macro_use]
mod localize;
mod backend;
mod view;
use backend::{ToplevelInfo, ZcosmicToplevelHandleV1, ZcosmicWorkspaceHandleV1};
mod utils;
mod widgets;

#[derive(Clone, Debug, Default, PartialEq, CosmicConfigEntry)]
struct CosmicWorkspacesConfig {
    show_workspace_number: bool,
    show_workspace_name: bool,
}

// Include `pid` in mime. Want to drag between our surfaces, but not another
// process, if we use Wayland object ids.
#[allow(dead_code)]
static WORKSPACE_MIME: LazyLock<String> =
    LazyLock::new(|| format!("text/x.cosmic-workspace-id-{}", std::process::id()));

static TOPLEVEL_MIME: LazyLock<String> =
    LazyLock::new(|| format!("text/x.cosmic-toplevel-id-{}", std::process::id()));

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

// TODO store protocol object id?
#[derive(Clone, Debug)]
struct DragToplevel {}

impl AsMimeTypes for DragToplevel {
    fn available(&self) -> Cow<'static, [String]> {
        vec![TOPLEVEL_MIME.clone()].into()
    }

    fn as_bytes(&self, mime_type: &str) -> Option<Cow<'static, [u8]>> {
        if mime_type == *TOPLEVEL_MIME {
            Some(Vec::new().into())
        } else {
            None
        }
    }
}

impl cosmic::iced::clipboard::mime::AllowedMimeTypes for DragToplevel {
    fn allowed() -> Cow<'static, [String]> {
        vec![crate::TOPLEVEL_MIME.clone()].into()
    }
}

impl TryFrom<(Vec<u8>, std::string::String)> for DragToplevel {
    type Error = ();
    fn try_from((_bytes, mime_type): (Vec<u8>, String)) -> Result<Self, ()> {
        if mime_type == *TOPLEVEL_MIME {
            Ok(Self {})
        } else {
            Err(())
        }
    }
}

#[derive(Clone, Debug)]
enum Msg {
    WaylandEvent(WaylandEvent),
    Wayland(backend::Event),
    Close,
    ActivateWorkspace(ZcosmicWorkspaceHandleV1),
    #[allow(dead_code)]
    CloseWorkspace(ZcosmicWorkspaceHandleV1),
    ActivateToplevel(ZcosmicToplevelHandleV1),
    CloseToplevel(ZcosmicToplevelHandleV1),
    //StartDrag(Size, Vector, DragSurface),
    StartDrag(DragSurface),
    DndWorkspaceEnter(
        ZcosmicWorkspaceHandleV1,
        wl_output::WlOutput,
        f64,
        f64,
        Vec<String>,
    ),
    DndWorkspaceLeave(ZcosmicWorkspaceHandleV1, wl_output::WlOutput),
    DndWorkspaceDrop(DragToplevel),
    SourceFinished,
    #[allow(dead_code)]
    NewWorkspace,
    CompConfig(Box<CosmicCompConfig>),
    Config(CosmicWorkspacesConfig),
    BgConfig(cosmic_bg_config::state::State),
    Ignore,
}

#[derive(Debug)]
struct Workspace {
    name: String,
    img_for_output: HashMap<wl_output::WlOutput, backend::CaptureImage>,
    handle: ZcosmicWorkspaceHandleV1,
    outputs: HashSet<wl_output::WlOutput>,
    is_active: bool,
}

#[derive(Clone, Debug)]
struct Toplevel {
    handle: ZcosmicToplevelHandleV1,
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

#[derive(Clone, Debug)]
enum DragSurface {
    #[allow(dead_code)]
    Workspace {
        handle: ZcosmicWorkspaceHandleV1,
        output: wl_output::WlOutput,
    },
    Toplevel {
        handle: ZcosmicToplevelHandleV1,
        output: wl_output::WlOutput,
    },
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
    drop_target: Option<(ZcosmicWorkspaceHandleV1, wl_output::WlOutput)>,
}

impl App {
    fn workspace_for_handle(&self, handle: &ZcosmicWorkspaceHandleV1) -> Option<&Workspace> {
        self.workspaces.iter().find(|i| &i.handle == handle)
    }

    fn workspace_for_handle_mut(
        &mut self,
        handle: &ZcosmicWorkspaceHandleV1,
    ) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|i| &i.handle == handle)
    }

    fn toplevel_for_handle_mut(
        &mut self,
        handle: &ZcosmicToplevelHandleV1,
    ) -> Option<&mut Toplevel> {
        self.toplevels.iter_mut().find(|i| &i.handle == handle)
    }

    fn create_surface(&mut self, output: wl_output::WlOutput) -> Task<cosmic::app::Message<Msg>> {
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

    fn destroy_surface(&mut self, output: &wl_output::WlOutput) -> Task<cosmic::app::Message<Msg>> {
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

    fn toggle(&mut self) -> Task<cosmic::app::Message<Msg>> {
        if self.visible {
            self.hide()
        } else {
            self.show()
        }
    }

    fn show(&mut self) -> Task<cosmic::app::Message<Msg>> {
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
    fn hide(&mut self) -> Task<cosmic::app::Message<Msg>> {
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

    fn init(core: cosmic::app::Core, _flags: Self::Flags) -> (Self, Task<Message<Self::Message>>) {
        (
            Self {
                core,
                ..Default::default()
            },
            Task::none(),
        )
    }
    // TODO: show panel and dock? Drag?

    fn update(&mut self, message: Msg) -> Task<cosmic::app::Message<Msg>> {
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
                    backend::Event::Workspaces(workspaces) => {
                        let old_workspaces = mem::take(&mut self.workspaces);
                        self.workspaces = Vec::new();
                        for (outputs, workspace) in workspaces {
                            let is_active = workspace.state.contains(&WEnum::Value(
                                zcosmic_workspace_handle_v1::State::Active,
                            ));

                            // XXX efficiency
                            #[allow(clippy::mutable_key_type)]
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
                    backend::Event::NewToplevel(handle, info) => {
                        log::debug!("New toplevel: {info:?}");
                        self.toplevels.push(Toplevel {
                            icon: desktop_info::icon_for_app_id(info.app_id.clone()),
                            handle,
                            info,
                            img: None,
                        });
                        // Close workspaces view if a window spawns while open
                        #[cfg(not(feature = "mock-backend"))]
                        if self.visible {
                            return self.hide();
                        }
                    }
                    backend::Event::UpdateToplevel(handle, info) => {
                        if let Some(toplevel) =
                            self.toplevels.iter_mut().find(|x| x.handle == handle)
                        {
                            if toplevel.info.app_id != info.app_id {
                                toplevel.icon = desktop_info::icon_for_app_id(info.app_id.clone());
                            }
                            toplevel.info = info;
                        }
                    }
                    backend::Event::CloseToplevel(handle) => {
                        if let Some(idx) = self.toplevels.iter().position(|x| x.handle == handle) {
                            self.toplevels.remove(idx);
                        }
                    }
                    backend::Event::WorkspaceCapture(handle, output_name, image) => {
                        //println!("Workspace capture");
                        if let Some(workspace) = self.workspace_for_handle_mut(&handle) {
                            workspace.img_for_output.insert(output_name, image);
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
            Msg::DndWorkspaceEnter(handle, output, _x, _y, _mimes) => {
                self.drop_target = Some((handle, output));
            }
            Msg::DndWorkspaceLeave(handle, output) => {
                // Currently in iced-sctk, a `DndOfferEvent::Motion` may cause a leave event after
                // an enter event, based on which widget handles it first. So we need a test here.
                if self.drop_target == Some((handle, output)) {
                    self.drop_target = None;
                }
            }
            Msg::DndWorkspaceDrop(_toplevel) => {
                if let Some((DragSurface::Toplevel { handle, .. }, _)) = &self.drag_surface {
                    if let Some(drop_target) = self.drop_target.take() {
                        self.send_wayland_cmd(backend::Cmd::MoveToplevelToWorkspace(
                            handle.clone(),
                            drop_target.0,
                            drop_target.1,
                        ));
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
            Msg::Ignore => {}
        }

        Task::none()
    }
    fn dbus_activation(
        &mut self,
        msg: cosmic::app::DbusActivationMessage,
    ) -> Task<cosmic::app::Message<Self::Message>> {
        if let DbusActivationDetails::Activate = msg.msg {
            self.toggle()
        } else {
            Task::none()
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        let events = iced::event::listen_with(|evt, _, _| {
            if let iced::Event::PlatformSpecific(iced::event::PlatformSpecific::Wayland(evt)) = evt
            {
                Some(Msg::WaylandEvent(evt))
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
