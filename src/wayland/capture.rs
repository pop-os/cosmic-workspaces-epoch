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

#[derive(Clone, PartialEq, Eq)]
pub enum CaptureSource {
    Toplevel(zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1),
    Workspace(
        zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
        wl_output::WlOutput,
    ),
}

#[allow(clippy::derive_hash_xor_eq)]
impl std::hash::Hash for CaptureSource {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        match self {
            Self::Toplevel(handle) => handle.id(),
            Self::Workspace(handle, _output) => handle.id(),
        }
        .hash(state)
    }
}

#[derive(Clone, Debug, Default)]
pub struct CaptureFilter {
    pub workspaces_on_outputs: Vec<wl_output::WlOutput>,
    pub toplevels_on_workspaces: Vec<zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1>,
}

pub struct Capture {
    pub buffer: Mutex<Option<Buffer>>,
    pub source: CaptureSource,
    pub first_frame: AtomicBool,
    pub cancelled: AtomicBool,
}

impl Capture {
    pub fn new(source: CaptureSource) -> Capture {
        Capture {
            buffer: Mutex::new(None),
            source,
            first_frame: AtomicBool::new(true),
            cancelled: AtomicBool::new(false),
        }
    }

    pub fn for_session(
        session: &zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
    ) -> Option<&Arc<Self>> {
        Some(&session.data::<SessionData>()?.capture)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn capture(
        self: &Arc<Self>,
        manager: &zcosmic_screencopy_manager_v1::ZcosmicScreencopyManagerV1,
        qh: &QueueHandle<AppData>,
    ) {
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
