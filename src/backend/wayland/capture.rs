use cctk::{
    screencopy::{CaptureSession, CaptureSource, ScreencopyState},
    wayland_client::QueueHandle,
};
use cosmic::cctk;

use std::sync::{Arc, Mutex};

use super::{AppData, ScreencopySession, SessionData};

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
    pub fn for_session(session: &CaptureSession) -> Option<Arc<Self>> {
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
