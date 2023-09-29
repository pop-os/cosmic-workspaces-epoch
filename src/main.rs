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
        compositor::{CompositorHandler, CompositorState},
        output::{OutputHandler, OutputState},
        reexports::calloop_wayland_source::WaylandSource,
        registry::{ProvidesRegistryState, RegistryState, SimpleGlobal},
        shell::{
            wlr_layer::{
                Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
                LayerSurfaceConfigure,
            },
            xdg::window::{Window, WindowConfigure, WindowHandler},
        },
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
// XXX
mod mpsc;

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

struct LayerSurfaceInstance {
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
    layer_shell: LayerShell,
    compositor_state: CompositorState,
    visible: bool,
    qh: QueueHandle<Self>,
}

sctk::delegate_compositor!(App);
sctk::delegate_output!(App);

sctk::delegate_xdg_shell!(App);
sctk::delegate_xdg_window!(App);
sctk::delegate_layer!(App);

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

impl LayerShellHandler for App {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {}

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
    }
}

impl App {
    fn handle_wayland_event(&mut self, event: wayland::Event) {
        match event {
            wayland::Event::Connection(_conn) => {}
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

    fn toggle(&mut self) {
        println!("Toggle!");
        self.visible = !self.visible;
        if self.visible {
            for output in self.output_state.outputs() {
                let surface = self.compositor_state.create_surface(&self.qh);
                self.layer_shell.create_layer_surface(
                    &self.qh,
                    surface,
                    Layer::Overlay,
                    Some("cosmic-workspaces"),
                    Some(&output),
                );
            }
            // TODO create shell surfaces
        } else {
            // TODO close shell surfaces
        }
    }
}

fn main() {
    let mut toggles = toggle_dbus::stream();
    let conn = Connection::connect_to_env().unwrap();
    let mut events = wayland::start(conn.clone());

    let (globals, event_queue) = registry_queue_init::<App>(&conn).unwrap();
    let qh = event_queue.handle();
    let registry_state = RegistryState::new(&globals);
    let wp_viewporter = SimpleGlobal::<wp_viewporter::WpViewporter, 1>::bind(&globals, &qh)
        .unwrap()
        .get()
        .unwrap()
        .clone();
    let mut app: App = App {
        output_state: OutputState::new(&globals, &qh),
        registry_state,
        wp_viewporter,
        layer_shell: LayerShell::bind(&globals, &qh).unwrap(),
        compositor_state: CompositorState::bind(&globals, &qh).unwrap(),
        visible: true,
        qh: qh.clone(),
    };
    let mut event_loop = calloop::EventLoop::try_new().unwrap();
    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .unwrap();
    event_loop
        .handle()
        .insert_source(toggles, |_, _, app| {
            app.toggle();
        })
        .unwrap();
    event_loop
        .handle()
        .insert_source(events, |evt, (), app| {
            if let calloop::channel::Event::Msg(evt) = evt {
                app.handle_wayland_event(evt);
            }
        })
        .unwrap();
    loop {
        event_loop.dispatch(None, &mut app).unwrap();
    }
}
