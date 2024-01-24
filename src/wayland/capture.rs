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
    running: AtomicBool,
    capturing: AtomicBool,
    session: zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
}

impl Capture {
    pub fn new(
        source: CaptureSource,
        manager: &zcosmic_screencopy_manager_v1::ZcosmicScreencopyManagerV1,
        qh: &QueueHandle<AppData>,
    ) -> Arc<Capture> {
        Arc::new_cyclic(|weak_capture| {
            let udata = SessionData {
                session_data: Default::default(),
                capture: weak_capture.clone(),
            };

            let session = match &source {
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
            };

            Capture {
                buffer: Mutex::new(None),
                source,
                first_frame: AtomicBool::new(true),
                running: AtomicBool::new(false),
                capturing: AtomicBool::new(false),
                session,
            }
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
        self.running.load(Ordering::SeqCst)
    }

    // Buffer is currently attached and commited for capture by server
    pub fn capturing(&self) -> bool {
        self.capturing.load(Ordering::SeqCst)
    }

    pub fn set_capturing(&self, value: bool) {
        if value {
            self.first_frame.store(false, Ordering::SeqCst);
        }
        self.capturing.store(value, Ordering::SeqCst);
    }

    pub fn first_frame(&self) -> bool {
        self.first_frame.load(Ordering::SeqCst)
    }

    // Start capturing frames
    pub fn start(&self, conn: &Connection) {
        let already_running = self.running.swap(true, Ordering::SeqCst);
        let have_buffer = self.buffer.lock().unwrap().is_some();
        if have_buffer && !already_running {
            self.attach_buffer_and_commit(conn);
        }
    }

    // Stop capturing. Can be started again with `start`
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.first_frame.store(true, Ordering::SeqCst);
        // TODO: Reallocate buffers on re-start
        // *self.buffer.lock().unwrap() = None;
    }

    pub fn attach_buffer_and_commit(&self, conn: &Connection) {
        let buffer = self.buffer.lock().unwrap();
        let buffer = buffer.as_ref().unwrap();

        let node = buffer
            .node()
            .and_then(|x| x.to_str().map(|x| x.to_string()));

        self.session.attach_buffer(&buffer.buffer, node, 0); // XXX age?
        if self.first_frame() {
            self.session
                .commit(zcosmic_screencopy_session_v1::Options::empty());
        } else {
            self.session
                .commit(zcosmic_screencopy_session_v1::Options::OnDamage);
        }
        conn.flush().unwrap();

        self.set_capturing(true);
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.session.destroy();
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
