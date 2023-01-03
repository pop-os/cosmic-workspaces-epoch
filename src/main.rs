use cctk::{
    cosmic_protocols::{
        toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
        workspace::v1::client::zcosmic_workspace_handle_v1,
    },
    sctk::shell::layer::{Anchor, KeyboardInteractivity, Layer},
    toplevel_info::ToplevelInfo,
    wayland_client::protocol::wl_output,
};
use iced::{
    event::wayland::{Event as WaylandEvent, OutputEvent},
    keyboard::KeyCode,
    sctk_settings::InitialSurface,
    widget, Application, Command, Element, Subscription,
};
use iced_native::{
    command::platform_specific::wayland::layer_surface::{IcedOutput, SctkLayerSurfaceSettings},
    window::Id as SurfaceId,
};
use iced_sctk::{
    application::SurfaceIdWrapper,
    commands::layer_surface::{destroy_layer_surface, get_layer_surface},
};
use std::{collections::HashMap, process};

mod wayland;

#[derive(Clone, Debug)]
enum Msg {
    WaylandEvent(WaylandEvent),
    Wayland(wayland::Event),
    Close,
    Closed(SurfaceIdWrapper),
    ActivateWorkspace(zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1),
}

#[derive(Debug)]
struct Workspace {
    name: String,
    img: Option<iced::widget::image::Handle>,
    handle: zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
    output_name: Option<String>,
}

#[derive(Debug)]
struct Toplevel {
    handle: zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    info: ToplevelInfo,
    img: Option<iced::widget::image::Handle>,
}

struct LayerSurface {
    output: wl_output::WlOutput,
    output_name: Option<String>,
    active_workspace: Option<zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1>,
    // for transitions, would need windows in more than one workspace? But don't capture all of
    // them all the time every frame.
}

#[derive(Default)]
struct App {
    max_surface_id: usize,
    layer_surfaces: HashMap<SurfaceId, LayerSurface>,
    workspaces: Vec<Workspace>,
    toplevels: Vec<Toplevel>,
}

impl App {
    fn next_surface_id(&mut self) -> SurfaceId {
        self.max_surface_id += 1;
        SurfaceId::new(self.max_surface_id)
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
    // TODO way to activate w/ keybind, button

    fn update(&mut self, message: Msg) -> Command<Msg> {
        match message {
            Msg::WaylandEvent(evt) => match evt {
                WaylandEvent::Output(evt, output) => match evt {
                    OutputEvent::Created(Some(info)) => {
                        if let Some((width, height)) = info.logical_size {
                            let id = self.next_surface_id();
                            self.layer_surfaces.insert(
                                id.clone(),
                                LayerSurface {
                                    output: output.clone(),
                                    output_name: info.name,
                                    active_workspace: None,
                                },
                            );
                            return get_layer_surface(SctkLayerSurfaceSettings {
                                id,
                                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                                namespace: "workspaces".into(),
                                layer: Layer::Overlay,
                                size: Some((Some(width as _), Some(height as _))),
                                output: IcedOutput::Output(output),
                                ..Default::default()
                            });
                        }
                    }
                    OutputEvent::Removed => {
                        if let Some((id, _)) = self
                            .layer_surfaces
                            .iter()
                            .find(|(_id, surface)| &surface.output == &output)
                        {
                            let id = *id;
                            self.layer_surfaces.remove(&id).unwrap();
                        }
                    }
                    // TODO handle update/remove
                    _ => {}
                },
                _ => {}
            },
            Msg::Wayland(evt) => {
                match evt {
                    wayland::Event::Workspaces(workspaces) => {
                        // XXX efficiency
                        // XXX removal
                        self.workspaces = Vec::new();
                        for (output_name, workspace) in workspaces {
                            self.workspaces.push(Workspace {
                                name: workspace.name,
                                handle: workspace.handle,
                                output_name,
                                img: None,
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
                }
            }
            Msg::Close => {
                std::process::exit(0);
            }
            Msg::Closed(_) => {}
            Msg::ActivateWorkspace(workspace_handle) => {
                // XXX
                for workspace in &self.workspaces {
                    if &workspace.handle == &workspace_handle {
                        for surface in self.layer_surfaces.values_mut() {
                            if &surface.output_name == &workspace.output_name {
                                surface.active_workspace = Some(workspace_handle);
                                break;
                            }
                        }
                        break;
                    }
                }
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
        iced::Subscription::batch(vec![events, wayland::subscription().map(Msg::Wayland)])
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
        toplevel_previews(
            app.toplevels
                .iter()
                .filter(|i| i.info.workspace == surface.active_workspace)
        ),
    ]
    .height(iced::Length::Fill)
    .width(iced::Length::Fill)
    .into()
}

fn workspace_sidebar_entry(workspace: &Workspace) -> cosmic::Element<Msg> {
    // Indicate active workspace?
    widget::column![
        widget::button(widget::text("X")), // TODO close button
        widget::button(widget::Image::new(workspace.img.clone().unwrap_or_else(
            || widget::image::Handle::from_pixels(0, 0, vec![0, 0, 0, 255])
        )))
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
    widget::button(widget::Image::new(toplevel.img.clone().unwrap_or_else(
        || widget::image::Handle::from_pixels(0, 0, vec![0, 0, 0, 255]),
    )))
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
