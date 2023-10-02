use cosmic::iced::futures::{executor::block_on, stream::StreamExt};
use std::{collections::HashMap, mem};
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
            WaylandSurface,
        },
        shm::{raw::RawPool, slot::SlotPool, Shm, ShmHandler},
    },
    toplevel_info::ToplevelInfo,
    wayland_client::{
        delegate_noop,
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
    img_for_output: HashMap<String, wl_buffer::WlBuffer>,
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
    layer_surface: LayerSurface,
}

struct App {
    output_state: OutputState,
    registry_state: RegistryState,
    wp_viewporter: WpViewporter,
    layer_shell: LayerShell,
    compositor_state: CompositorState,
    shm_state: Shm,
    visible: bool,
    qh: QueueHandle<Self>,
    layer_surfaces: Vec<LayerSurfaceInstance>,
    pool: SlotPool,
    wayland_cmd_sender: Option<calloop::channel::Sender<wayland::Cmd>>,
    workspaces: Vec<Workspace>,
}

sctk::delegate_compositor!(App);
sctk::delegate_output!(App);

sctk::delegate_xdg_shell!(App);
sctk::delegate_xdg_window!(App);
sctk::delegate_layer!(App);
sctk::delegate_shm!(App);

sctk::delegate_registry!(App);

sctk::delegate_simple!(App, WpViewporter, 1);

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

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
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let (width, height) = configure.new_size;
        let mut pool = RawPool::new(width as usize * height as usize * 4, &self.shm_state).unwrap();
        let mmap = pool.mmap();
        for y in 0..height {
            for x in 0..width {
                let offset = (y * width * 4 + x * 4) as usize;
                mmap[offset + 0] = 128;
                mmap[offset + 1] = 128;
                mmap[offset + 2] = 128;
                mmap[offset + 3] = 255;
            }
        }
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            width as i32 * 4,
            wl_shm::Format::Argb8888,
            (),
            qh,
        );
        layer.attach(Some(&buffer), 0, 0);
        layer.commit();
        println!("{:?}", configure);
    }
}

delegate_noop!(App: ignore wl_buffer::WlBuffer);

impl App {
    fn handle_wayland_event(&mut self, event: wayland::Event) {
        match event {
            wayland::Event::Connection(_conn) => {}
            wayland::Event::CmdSender(sender) => {
                self.wayland_cmd_sender = Some(sender);
            }
            wayland::Event::ToplevelManager(manager) => {}
            wayland::Event::WorkspaceManager(manager) => {}
            wayland::Event::Workspaces(workspaces) => {
                let old_workspaces = mem::take(&mut self.workspaces);
                self.workspaces = Vec::new();
                for (output_names, workspace) in workspaces {
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
                        output_names,
                        img_for_output,
                        is_active,
                    });
                }
                self.update_capture_filter();
            }
            wayland::Event::NewToplevel(handle, output_name, info) => {}
            wayland::Event::UpdateToplevel(handle, output_name, info) => {}
            wayland::Event::CloseToplevel(handle) => {}
            wayland::Event::WorkspaceCapture(handle, output_name, image) => {
                if let Some(workspace) = self.workspace_for_handle_mut(&handle) {
                    workspace.img_for_output.insert(output_name, image);
                }
                println!("workspace captured");
            }
            wayland::Event::ToplevelCapture(handle, image) => {}
            wayland::Event::Seats(seats) => {}
        }
    }

    fn workspace_for_handle_mut(
        &mut self,
        handle: &zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
    ) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|i| &i.handle == handle)
    }

    fn update_capture_filter(&self) {
        if let Some(sender) = self.wayland_cmd_sender.as_ref() {
            let mut capture_filter = wayland::CaptureFilter::default();
            if self.visible {
                // XXX handle on wrong connection
                capture_filter.workspaces_on_outputs = self
                    .layer_surfaces
                    .iter()
                    .map(|x| x.output_name.clone())
                    .collect();
                //    self.outputs.iter().map(|x| x.name.clone()).collect();
                // TODO
                /*
                capture_filter.toplevels_on_workspaces = self
                    .workspaces
                    .iter()
                    .filter(|x| x.is_active)
                    .map(|x| x.handle.clone())
                    .collect();
                */
            }
            let _ = sender.send(wayland::Cmd::CaptureFilter(capture_filter));
        }
    }

    fn toggle(&mut self) {
        println!("Toggle!");
        self.visible = !self.visible;
        if self.visible {
            for output in self.output_state.outputs() {
                if let Some(info) = self.output_state.info(&output) {
                    if let Some((width, height)) = info.logical_size {
                        let surface = self.compositor_state.create_surface(&self.qh);
                        let layer_surface = self.layer_shell.create_layer_surface(
                            &self.qh,
                            surface,
                            Layer::Overlay,
                            Some("cosmic-workspaces"),
                            Some(&output),
                        );
                        layer_surface.set_anchor(Anchor::all());
                        layer_surface.set_size(width as u32, height as u32);
                        layer_surface.commit();
                        self.layer_surfaces.push(LayerSurfaceInstance {
                            output,
                            output_name: info.name.unwrap_or_default(),
                            layer_surface,
                        });
                    }
                }
            }
            // TODO set filter
            // TODO create shell surfaces
        } else {
            // TODO close shell surfaces
            self.layer_surfaces.clear();
        }

        self.update_capture_filter();
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
    let shm_state = Shm::bind(&globals, &qh).unwrap();
    let mut app: App = App {
        output_state: OutputState::new(&globals, &qh),
        registry_state,
        wp_viewporter,
        layer_shell: LayerShell::bind(&globals, &qh).unwrap(),
        compositor_state: CompositorState::bind(&globals, &qh).unwrap(),
        visible: false,
        qh: qh.clone(),
        layer_surfaces: Vec::new(),
        pool: SlotPool::new(256 * 256 * 4, &shm_state).unwrap(),
        shm_state,
        wayland_cmd_sender: None,
        workspaces: Vec::new(),
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
