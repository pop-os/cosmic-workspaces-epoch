use cctk::{
    cosmic_protocols::{
        toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
        toplevel_management::v1::client::zcosmic_toplevel_manager_v1,
        workspace::v1::client::{zcosmic_workspace_handle_v1, zcosmic_workspace_manager_v1},
    },
    sctk::shell::layer::{Anchor, KeyboardInteractivity, Layer},
    toplevel_info::ToplevelInfo,
    wayland_client::{
        protocol::{wl_output, wl_seat},
        Connection, QueueHandle, WEnum,
    },
};
use cosmic::{
    iced::{
        self,
        event::wayland::{Event as WaylandEvent, OutputEvent},
        keyboard::KeyCode,
        widget, Application, Command, Element, Subscription,
    },
    iced_native::{
        command::platform_specific::wayland::layer_surface::{
            IcedOutput, SctkLayerSurfaceSettings,
        },
        window::Id as SurfaceId,
    },
    iced_sctk::{
        application::SurfaceIdWrapper,
        commands::layer_surface::{destroy_layer_surface, get_layer_surface},
        settings::InitialSurface,
    },
};
use std::{collections::HashMap, mem, process};

mod toggle_dbus;
mod wayland;

#[derive(Clone, Debug)]
enum Msg {
    WaylandEvent(WaylandEvent),
    Wayland(wayland::Event),
    Close,
    Closed(SurfaceIdWrapper),
    ActivateWorkspace(zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1),
    CloseWorkspace(zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1),
    ActivateToplevel(zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1),
    CloseToplevel(zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1),
    DBus(toggle_dbus::Event),
}

#[derive(Debug)]
struct Workspace {
    name: String,
    img: Option<iced::widget::image::Handle>,
    handle: zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
    output_name: Option<String>,
    is_active: bool,
}

#[derive(Debug)]
struct Toplevel {
    handle: zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    info: ToplevelInfo,
    img: Option<iced::widget::image::Handle>,
}

#[derive(Clone)]
struct Output {
    // Output, on the `iced_sctk` Wayland connection
    handle: wl_output::WlOutput,
    name: Option<String>,
    width: i32,
    height: i32,
}

struct LayerSurface {
    // Output, on the `iced_sctk` Wayland connection
    output: wl_output::WlOutput,
    output_name: Option<String>,
    // for transitions, would need windows in more than one workspace? But don't capture all of
    // them all the time every frame.
}

#[derive(Default)]
struct App {
    max_surface_id: usize,
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
}

impl App {
    fn next_surface_id(&mut self) -> SurfaceId {
        self.max_surface_id += 1;
        SurfaceId::new(self.max_surface_id)
    }

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
        output_name: Option<String>,
        width: i32,
        height: i32,
    ) -> Command<Msg> {
        let id = self.next_surface_id();
        self.layer_surfaces.insert(
            id.clone(),
            LayerSurface {
                output: output.clone(),
                output_name,
            },
        );
        get_layer_surface(SctkLayerSurfaceSettings {
            id,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            namespace: "workspace-overview".into(),
            layer: Layer::Overlay,
            size: Some((Some(width as _), Some(height as _))),
            output: IcedOutput::Output(output),
            ..Default::default()
        })
    }

    fn destroy_surface(&mut self, output: &wl_output::WlOutput) -> Command<Msg> {
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

    fn toggle(&mut self) -> Command<Msg> {
        if self.visible {
            self.hide()
        } else {
            self.show()
        }
    }

    fn show(&mut self) -> Command<Msg> {
        if !self.visible {
            self.visible = true;
            let outputs = self.outputs.clone();
            Command::batch(
                outputs
                    .into_iter()
                    .map(|output| {
                        self.create_surface(output.handle, output.name, output.width, output.height)
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            Command::none()
        }
    }

    // Close all shell surfaces
    fn hide(&mut self) -> Command<Msg> {
        self.visible = false;
        Command::batch(
            mem::take(&mut self.layer_surfaces)
                .into_keys()
                .map(destroy_layer_surface)
                .collect::<Vec<_>>(),
        )
    }
}

impl Application for App {
    type Message = Msg;
    type Theme = cosmic::Theme;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Msg>) {
        //(Self::default(), destroy_layer_surface(SurfaceId::new(0)))
        (Self::default(), Command::none())
    }

    fn title(&self) -> String {
        String::from("cosmic-workspaces")
    }

    // TODO transparent style?
    // TODO: show panel and dock? Drag?

    fn update(&mut self, message: Msg) -> Command<Msg> {
        match message {
            Msg::WaylandEvent(evt) => match evt {
                WaylandEvent::Output(evt, output) => match evt {
                    OutputEvent::Created(Some(info)) => {
                        if let Some((width, height)) = info.logical_size {
                            self.outputs.push(Output {
                                handle: output.clone(),
                                name: info.name.clone(),
                                width,
                                height,
                            });
                            if self.visible {
                                return self.create_surface(
                                    output.clone(),
                                    info.name,
                                    width,
                                    height,
                                );
                            }
                        }
                    }
                    OutputEvent::Created(None) => {} // XXX?
                    OutputEvent::InfoUpdate(info) => {
                        if let Some(output) = self.outputs.iter_mut().find(|x| &x.handle == &output)
                        {
                            if let Some((width, height)) = info.logical_size {
                                output.width = width;
                                output.height = height;
                            }
                            output.name = info.name;
                            // XXX re-create surface?
                        }
                    }
                    OutputEvent::Removed => {
                        if let Some(idx) = self.outputs.iter().position(|x| &x.handle == &output) {
                            self.outputs.remove(idx);
                        }
                        if self.visible {
                            return self.destroy_surface(&output);
                        }
                    }
                },
                _ => {}
            },
            Msg::Wayland(evt) => {
                match evt {
                    wayland::Event::Connection(conn) => {
                        self.conn = Some(conn);
                    }
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
                        for (output_name, workspace) in workspaces {
                            let is_active = workspace.state.contains(&WEnum::Value(
                                zcosmic_workspace_handle_v1::State::Active,
                            ));

                            // XXX efficiency
                            let img = old_workspaces
                                .iter()
                                .find(|i| &i.handle == &workspace.handle)
                                .and_then(|i| i.img.clone());

                            self.workspaces.push(Workspace {
                                name: workspace.name,
                                handle: workspace.handle,
                                output_name,
                                img,
                                is_active,
                            });
                        }
                    }
                    wayland::Event::NewToplevel(handle, info) => {
                        println!("New toplevel: {:?}", info);
                        self.toplevels.push(Toplevel {
                            handle,
                            info,
                            img: None,
                        });
                    }
                    wayland::Event::CloseToplevel(handle) => {
                        if let Some(idx) = self.toplevels.iter().position(|x| x.handle == handle) {
                            self.toplevels.remove(idx);
                        }
                    }
                    wayland::Event::WorkspaceCapture(handle, image) => {
                        if let Some(workspace) = self.workspace_for_handle_mut(&handle) {
                            workspace.img = Some(image.clone());
                        }
                    }
                    wayland::Event::ToplevelCapture(handle, image) => {
                        if let Some(toplevel) = self.toplevel_for_handle_mut(&handle) {
                            println!("Got toplevel image!");
                            toplevel.img = Some(image.clone());
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
                self.conn.as_ref().unwrap().flush();
            }
            Msg::ActivateToplevel(toplevel_handle) => {
                if let Some(toplevel_manager) = self.toplevel_manager.as_ref() {
                    if !self.seats.is_empty() {
                        for seat in &self.seats {
                            toplevel_manager.activate(&toplevel_handle, &seat);
                            self.conn.as_ref().unwrap().flush();
                        }
                        return self.hide();
                    }
                }
            }
            Msg::CloseWorkspace(workspace_handle) => {}
            Msg::CloseToplevel(toplevel_handle) => {
                // TODO confirmation?
                if let Some(toplevel_manager) = self.toplevel_manager.as_ref() {
                    toplevel_manager.close(&toplevel_handle);
                }
            }
            Msg::DBus(toggle_dbus::Event::Toggle) => {
                return self.toggle();
            }
        }

        Command::none()
    }

    fn subscription(&self) -> Subscription<Msg> {
        let events = iced::subscription::events_with(|evt, _| {
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
        iced::Subscription::batch(vec![
            events,
            toggle_dbus::subscription().map(Msg::DBus),
            wayland::subscription().map(Msg::Wayland),
        ])
    }

    fn view(&self, id: SurfaceIdWrapper) -> cosmic::Element<Msg> {
        use iced::widget::*;
        if let SurfaceIdWrapper::LayerSurface(id) = id {
            if let Some(surface) = self.layer_surfaces.get(&id) {
                return layer_surface(self, surface);
            }
        };
        text("workspaces").into()
    }

    fn close_requested(&self, id: SurfaceIdWrapper) -> Msg {
        Msg::Closed(id)
    }
}

fn layer_surface<'a>(app: &'a App, surface: &'a LayerSurface) -> cosmic::Element<'a, Msg> {
    widget::row![
        workspaces_sidebar(
            app.workspaces
                .iter()
                .filter(|i| &i.output_name == &surface.output_name),
        ),
        toplevel_previews(app.toplevels.iter().filter(|i| {
            if let Some(workspace) = &i.info.workspace {
                app.workspace_for_handle(workspace).map_or(false, |x| {
                    x.is_active && x.output_name == surface.output_name
                })
            } else {
                false
            }
        })),
    ]
    .height(iced::Length::Fill)
    .width(iced::Length::Fill)
    .into()
}

fn close_button(on_press: Msg) -> cosmic::Element<'static, Msg> {
    iced::widget::button(cosmic::widget::icon("window-close-symbolic", 16))
        .style(cosmic::theme::Button::Destructive)
        .on_press(on_press)
        .into()
}

fn workspace_sidebar_entry(workspace: &Workspace) -> cosmic::Element<Msg> {
    // TODO style
    let theme = if workspace.is_active {
        cosmic::theme::Button::Primary
    } else {
        cosmic::theme::Button::Secondary
    };
    widget::column![
        close_button(Msg::CloseWorkspace(workspace.handle.clone())),
        widget::button(widget::Image::new(workspace.img.clone().unwrap_or_else(
            || widget::image::Handle::from_pixels(1, 1, vec![0, 0, 0, 255])
        )))
        .style(theme)
        .on_press(Msg::ActivateWorkspace(workspace.handle.clone())),
        widget::text(&workspace.name)
    ]
    .into()
}

fn workspaces_sidebar<'a>(
    workspaces: impl Iterator<Item = &'a Workspace>,
) -> cosmic::Element<'a, Msg> {
    widget::column(workspaces.map(workspace_sidebar_entry).collect())
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into()

    // New workspace
}

fn toplevel_preview<'a>(toplevel: &'a Toplevel) -> cosmic::Element<'a, Msg> {
    // capture of window
    // - selectable
    // name of window
    widget::column![
        close_button(Msg::CloseToplevel(toplevel.handle.clone())),
        widget::button(widget::Image::new(toplevel.img.clone().unwrap_or_else(
            || widget::image::Handle::from_pixels(1, 1, vec![0, 0, 0, 255]),
        )))
        .on_press(Msg::ActivateToplevel(toplevel.handle.clone())),
        widget::text(&toplevel.info.title)
    ]
    .into()
}

fn toplevel_previews<'a>(
    toplevels: impl Iterator<Item = &'a Toplevel>,
) -> cosmic::Element<'a, Msg> {
    widget::row(toplevels.map(toplevel_preview).collect())
        .width(iced::Length::FillPortion(4))
        .height(iced::Length::Fill)
        .into()
}

pub fn main() -> iced::Result {
    App::run(iced::Settings {
        antialiasing: true,
        exit_on_close_request: false,
        initial_surface: InitialSurface::LayerSurface(SctkLayerSurfaceSettings {
            keyboard_interactivity: KeyboardInteractivity::None,
            namespace: "ignore".into(),
            size: Some((Some(1), Some(1))),
            layer: Layer::Background,
            ..Default::default()
        }),
        ..iced::Settings::default()
    })
}
