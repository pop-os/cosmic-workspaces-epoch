use cosmic::iced::futures::{executor::block_on, stream::StreamExt};
use std::collections::HashMap;
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::{self, WpViewport},
    wp_viewporter::{self, WpViewporter},
};

use cctk::{
    cosmic_protocols::{
        toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
        toplevel_management::v1::client::zcosmic_toplevel_manager_v1,
        workspace::v1::client::{zcosmic_workspace_handle_v1, zcosmic_workspace_manager_v1},
    },
    sctk::{
        self,
        compositor::CompositorHandler,
        output::{OutputHandler, OutputState},
        registry::{ProvidesRegistryState, RegistryState},
        shell::xdg::window::{Window, WindowConfigure, WindowHandler},
    },
    toplevel_info::ToplevelInfo,
    wayland_client::{
        globals::registry_queue_init,
        protocol::{
            wl_buffer, wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_subsurface,
            wl_surface,
        },
        Connection, Dispatch, Proxy, QueueHandle, WEnum,
    },
};

mod toggle_dbus;
mod wayland;

#[derive(Debug)]
struct Workspace {
    name: String,
    //img_for_output: HashMap<String, iced::widget::image::Handle>,
    handle: zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
    output_names: Vec<String>,
    is_active: bool,
}

#[derive(Debug)]
struct Toplevel {
    handle: zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    info: ToplevelInfo,
    output_name: Option<String>,
    // img: Option<iced::widget::image::Handle>,
}

#[derive(Clone)]
struct Output {
    // Output, on the `iced_sctk` Wayland connection
    handle: wl_output::WlOutput,
    name: String,
    width: i32,
    height: i32,
}

struct LayerSurface {
    // Output, on the `iced_sctk` Wayland connection
    output: wl_output::WlOutput,
    output_name: String,
    // for transitions, would need windows in more than one workspace? But don't capture all of
    // them all the time every frame.
}

struct App {
    output_state: OutputState,
    registry_state: RegistryState,
    wp_viewporter: WpViewporter,
}

fn main() {
    let mut events = wayland::start();
    block_on(async {
        while let Some(evt) = events.next().await {
            match evt {
                wayland::Event::Connection(conn) => {}
                wayland::Event::CmdSender(sender) => {}
                wayland::Event::ToplevelManager(manager) => {}
                wayland::Event::WorkspaceManager(manager) => {}
                wayland::Event::Workspaces(workspaces) => {}
                wayland::Event::NewToplevel(handle, output_name, info) => {}
                wayland::Event::UpdateToplevel(handle, output_name, info) => {}
                wayland::Event::CloseToplevel(handle) => {}
                wayland::Event::WorkspaceCapture(handle, output_name, image) => {}
                wayland::Event::ToplevelCapture(handle, image) => {}
                wayland::Event::Seats(seats) => {}
            }
        }
    })
}

sctk::delegate_compositor!(App);
sctk::delegate_output!(App);

sctk::delegate_xdg_shell!(App);
sctk::delegate_xdg_window!(App);

sctk::delegate_registry!(App);

sctk::delegate_simple!(App, WpViewporter, 1);

impl Dispatch<WpViewport, ()> for App {
    fn event(
        _: &mut App,
        _: &WpViewport,
        _: wp_viewport::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        unreachable!("wp_viewport::Event is empty in version 1")
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
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
        _output: wl_output::WlOutput,
    ) {
    }
}

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {}

    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        _window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    sctk::registry_handlers![OutputState,];
}
