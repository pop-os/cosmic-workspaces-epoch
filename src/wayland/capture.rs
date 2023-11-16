use cctk::{
    cosmic_protocols::{
        screencopy::v1::client::{zcosmic_screencopy_manager_v1, zcosmic_screencopy_session_v1},
        toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
        workspace::v1::client::zcosmic_workspace_handle_v1,
    },
    screencopy::{ScreencopySessionData, ScreencopySessionDataExt},
    wayland_client::{protocol::wl_output, Proxy, QueueHandle},
};

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use super::{AppData, Buffer};

#[derive(Clone, Hash, PartialEq, Eq)]
pub enum CaptureSource {
    Toplevel(zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1),
    Workspace(
        zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
        wl_output::WlOutput,
    ),
}

#[derive(Clone, Debug, Default)]
pub struct CaptureFilter {
    // TODO: Use `WlOutput` when one Wayland connection is used
    pub workspaces_on_outputs: Vec<wl_output::WlOutput>,
    pub toplevels_on_workspaces: Vec<zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1>,
}

pub struct Capture {
    pub buffer: Mutex<Option<Buffer>>,
    pub source: CaptureSource,
    first_frame: AtomicBool,
    running: AtomicBool,
}

impl Capture {
    pub fn new(source: CaptureSource) -> Capture {
        Capture {
            buffer: Mutex::new(None),
            source,
            first_frame: AtomicBool::new(true),
            running: AtomicBool::new(false),
        }
    }

    pub fn for_session(
        session: &zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
    ) -> Option<&Arc<Self>> {
        Some(&session.data::<SessionData>()?.capture)
    }

    pub fn running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn first_frame(&self) -> bool {
        self.first_frame.load(Ordering::SeqCst)
    }

    pub fn cancel(&self) {
        self.running.store(false, Ordering::SeqCst);
        *self.buffer.lock().unwrap() = None;
    }

    pub fn capture(
        self: &Arc<Self>,
        manager: &zcosmic_screencopy_manager_v1::ZcosmicScreencopyManagerV1,
        qh: &QueueHandle<AppData>,
    ) {
        // Mark as running. If already running, this is not the first frame.
        let already_running = self.running.swap(true, Ordering::SeqCst);
        self.first_frame.store(!already_running, Ordering::SeqCst);

        let udata = SessionData {
            session_data: Default::default(),
            capture: self.clone(),
        };
        match &self.source {
            CaptureSource::Toplevel(toplevel) => {
                manager.capture_toplevel(
                    toplevel,
                    zcosmic_screencopy_manager_v1::CursorMode::Hidden,
                    qh,
                    udata,
                );
            }
            CaptureSource::Workspace(workspace, output) => {
                manager.capture_workspace(
                    workspace,
                    output,
                    zcosmic_screencopy_manager_v1::CursorMode::Hidden,
                    qh,
                    udata,
                );
            }
        }
    }
}

struct SessionData {
    session_data: ScreencopySessionData,
    capture: Arc<Capture>,
}

impl ScreencopySessionDataExt for SessionData {
    fn screencopy_session_data(&self) -> &ScreencopySessionData {
        &self.session_data
    }
}

cctk::delegate_screencopy!(AppData, session: [SessionData]);
