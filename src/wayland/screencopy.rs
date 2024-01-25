use cosmic::cctk::{
    self,
    cosmic_protocols::screencopy::v1::client::{
        zcosmic_screencopy_manager_v1, zcosmic_screencopy_session_v1,
    },
    screencopy::{
        BufferInfo, ScreencopyHandler, ScreencopySessionData, ScreencopySessionDataExt,
        ScreencopyState,
    },
    wayland_client::{Connection, QueueHandle, WEnum},
};
use std::{
    mem,
    sync::{Arc, Weak},
};

use super::{AppData, Buffer, Capture, CaptureImage, CaptureSource, Event};

pub struct ScreencopySession {
    buffers: Option<(Buffer, Buffer)>,
    session: zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
    first_frame: bool,
}

impl ScreencopySession {
    pub fn new(
        capture: &Arc<Capture>,
        manager: &zcosmic_screencopy_manager_v1::ZcosmicScreencopyManagerV1,
        qh: &QueueHandle<AppData>,
    ) -> Self {
        let udata = SessionData {
            session_data: Default::default(),
            capture: Arc::downgrade(capture),
        };

        let session = match &capture.source {
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

        Self {
            buffers: None,
            session,
            first_frame: true,
        }
    }

    fn attach_buffer_and_commit(&mut self, capture: &Capture, conn: &Connection) {
        let Some((front, back)) = self.buffers.as_mut() else {
            return;
        };

        let node = front.node().and_then(|x| x.to_str().map(|x| x.to_string()));

        self.session.attach_buffer(&back.buffer, node, 0); // XXX age?
        if self.first_frame {
            self.session
                .commit(zcosmic_screencopy_session_v1::Options::empty());
            self.first_frame = false;
        } else {
            self.session
                .commit(zcosmic_screencopy_session_v1::Options::OnDamage);
        }
        conn.flush().unwrap();
    }
}

impl Drop for ScreencopySession {
    fn drop(&mut self) {
        self.session.destroy();
    }
}

pub struct SessionData {
    session_data: ScreencopySessionData,
    // Weak reference so session can be destroyed when all strong references
    // are dropped.
    pub capture: Weak<Capture>,
}

impl ScreencopySessionDataExt for SessionData {
    fn screencopy_session_data(&self) -> &ScreencopySessionData {
        &self.session_data
    }
}

impl ScreencopyHandler for AppData {
    fn screencopy_state(&mut self) -> &mut ScreencopyState {
        &mut self.screencopy_state
    }

    fn init_done(
        &mut self,
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        session: &zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
        buffer_infos: &[BufferInfo],
    ) {
        let Some(capture) = Capture::for_session(session) else {
            return;
        };
        let mut session = capture.session.lock().unwrap();
        let Some(session) = session.as_mut() else {
            return;
        };

        // Create new buffer if none, or different format
        if !session.buffers.as_ref().map_or(false, |(front, _)| {
            buffer_infos.contains(&front.buffer_info)
        }) {
            session.buffers = Some((
                self.create_buffer(buffer_infos),
                self.create_buffer(buffer_infos),
            ));
        }

        session.attach_buffer_and_commit(&capture, conn);
    }

    fn ready(
        &mut self,
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        session: &zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
    ) {
        let Some(capture) = Capture::for_session(session) else {
            return;
        };
        let mut session = capture.session.lock().unwrap();
        let Some(session) = session.as_mut() else {
            return;
        };

        if session.buffers.is_none() {
            eprintln!("Error: No capture buffers?");
            return;
        }

        let (front, back) = session.buffers.as_mut().unwrap();
        mem::swap(front, back);

        let img = unsafe { front.to_image() };
        let image = CaptureImage { img };
        match &capture.source {
            CaptureSource::Toplevel(toplevel) => {
                self.send_event(Event::ToplevelCapture(toplevel.clone(), image))
            }
            CaptureSource::Workspace(workspace, output) => {
                self.send_event(Event::WorkspaceCapture(
                    workspace.clone(),
                    output.clone(),
                    image,
                ));
            }
        };

        // Capture again on damage
        session.attach_buffer_and_commit(&capture, conn);
    }

    fn failed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        session: &zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
        _reason: WEnum<zcosmic_screencopy_session_v1::FailureReason>,
    ) {
        // TODO
        println!("Failed");
        if let Some(capture) = Capture::for_session(session) {
            capture.stop();
        }
    }
}

cctk::delegate_screencopy!(AppData, session: [SessionData]);
