use cctk::{
    cosmic_protocols::{
        screencopy::v1::client::{zcosmic_screencopy_manager_v1, zcosmic_screencopy_session_v1},
        toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
        workspace::v1::client::zcosmic_workspace_handle_v1,
    },
    screencopy::{ScreencopySessionData, ScreencopySessionDataExt},
    wayland_client::{protocol::wl_output, Connection, Proxy, QueueHandle},
};
use cosmic::cctk;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, Weak,
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
    pub workspaces_on_outputs: Vec<wl_output::WlOutput>,
    pub toplevels_on_workspaces: Vec<zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1>,
}

pub struct Capture {
    pub buffer: Mutex<Option<Buffer>>,
    pub source: CaptureSource,
    first_frame: AtomicBool,
    session: Mutex<Option<zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1>>,
}

impl Capture {
    pub fn new(
        source: CaptureSource,
        manager: &zcosmic_screencopy_manager_v1::ZcosmicScreencopyManagerV1,
        qh: &QueueHandle<AppData>,
    ) -> Arc<Capture> {
        Arc::new(Capture {
            buffer: Mutex::new(None),
            source,
            first_frame: AtomicBool::new(true),
            session: Mutex::new(None),
        })
    }

    // Returns `None` if capture is destroyed
    // (or if `session` wasn't created with `SessionData`)
    pub fn for_session(
        session: &zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
    ) -> Option<Arc<Self>> {
        session.data::<SessionData>()?.capture.upgrade()
    }

    pub fn running(&self) -> bool {
        self.session.lock().unwrap().is_some()
    }

    pub fn first_frame(&self) -> bool {
        self.first_frame.load(Ordering::SeqCst)
    }

    // Start capturing frames
    pub fn start(
        self: &Arc<Self>,
        manager: &zcosmic_screencopy_manager_v1::ZcosmicScreencopyManagerV1,
        qh: &QueueHandle<AppData>,
    ) {
        let mut session = self.session.lock().unwrap();
        if session.is_none() {
            self.first_frame.store(true, Ordering::SeqCst);

            let udata = SessionData {
                session_data: Default::default(),
                capture: Arc::downgrade(self),
            };

            *session = Some(match &self.source {
                CaptureSource::Toplevel(toplevel) => manager.capture_toplevel(
                    toplevel,
                    zcosmic_screencopy_manager_v1::CursorMode::Hidden,
                    qh,
                    udata,
                ),
                CaptureSource::Workspace(workspace, output) => manager.capture_workspace(
                    workspace,
                    output,
                    zcosmic_screencopy_manager_v1::CursorMode::Hidden,
                    qh,
                    udata,
                ),
            });
        }
    }

    // Stop capturing. Can be started again with `start`
    pub fn stop(&self) {
        if let Some(session) = self.session.lock().unwrap().take() {
            session.destroy();
        }
        *self.buffer.lock().unwrap() = None;
    }

    pub fn attach_buffer_and_commit(&self, conn: &Connection) {
        let session = self.session.lock().unwrap();
        let buffer = self.buffer.lock().unwrap();
        let (Some(session), Some(buffer)) = (session.as_ref(), buffer.as_ref()) else {
            return;
        };

        let node = buffer
            .node()
            .and_then(|x| x.to_str().map(|x| x.to_string()));

        session.attach_buffer(&buffer.buffer, node, 0); // XXX age?
        if self.first_frame() {
            session.commit(zcosmic_screencopy_session_v1::Options::empty());
        } else {
            session.commit(zcosmic_screencopy_session_v1::Options::OnDamage);
        }
        conn.flush().unwrap();
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(session) = self.session.lock().unwrap().as_ref() {
            session.destroy();
        }
    }
}

struct SessionData {
    session_data: ScreencopySessionData,
    capture: Weak<Capture>,
}

impl ScreencopySessionDataExt for SessionData {
    fn screencopy_session_data(&self) -> &ScreencopySessionData {
        &self.session_data
    }
}

cctk::delegate_screencopy!(AppData, session: [SessionData]);
