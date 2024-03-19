use cctk::{
    cosmic_protocols::{
        screencopy::v2::client::{zcosmic_screencopy_manager_v2, zcosmic_screencopy_session_v2},
        toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
        workspace::v1::client::zcosmic_workspace_handle_v1,
    },
    screencopy::ScreencopyState,
    wayland_client::{protocol::wl_output, Proxy, QueueHandle},
};
use cosmic::cctk;

use std::sync::{Arc, Mutex};

use super::{AppData, ScreencopySession, SessionData};

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
    pub workspaces_on_outputs: Vec<wl_output::WlOutput>,
    pub toplevels_on_workspaces: Vec<zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1>,
}

pub struct Capture {
    pub source: CaptureSource,
    pub session: Mutex<Option<ScreencopySession>>,
}

impl Capture {
    pub fn new(source: CaptureSource) -> Arc<Capture> {
        Arc::new(Capture {
            source,
            session: Mutex::new(None),
        })
    }

    // Returns `None` if capture is destroyed
    // (or if `session` wasn't created with `SessionData`)
    pub fn for_session(
        session: &zcosmic_screencopy_session_v2::ZcosmicScreencopySessionV2,
    ) -> Option<Arc<Self>> {
        session.data::<SessionData>()?.capture.upgrade()
    }

    // Start capturing frames
    pub fn start(self: &Arc<Self>, screencopy_state: &ScreencopyState, qh: &QueueHandle<AppData>) {
        let mut session = self.session.lock().unwrap();
        if session.is_none() {
            *session = Some(ScreencopySession::new(self, screencopy_state, qh));
        }
    }

    // Stop capturing. Can be started again with `start`
    pub fn stop(&self) {
        self.session.lock().unwrap().take();
    }
}
