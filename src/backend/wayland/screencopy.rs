use cosmic::{
    cctk::{
        self,
        screencopy::{
            CaptureFrame, CaptureOptions, CaptureSession, CaptureSource, FailureReason, Formats,
            Frame, ScreencopyFrameData, ScreencopyFrameDataExt, ScreencopyHandler,
            ScreencopySessionData, ScreencopySessionDataExt, ScreencopyState,
        },
        wayland_client::{Connection, QueueHandle, WEnum},
    },
    iced_winit::platform_specific::wayland::subsurface_widget::{
        SubsurfaceBuffer, SubsurfaceBufferRelease,
    },
};
use std::{
    array,
    sync::{Arc, Weak},
};

use super::{AppData, Buffer, Capture, CaptureImage, Event};

pub struct ScreencopySession {
    // swapchain buffers
    buffers: Option<[Buffer; 2]>,
    session: CaptureSession,
    // Future signaled when buffer is signaled.
    // if triple buffer is used, will need more than one.
    release: Option<SubsurfaceBufferRelease>,
}

impl ScreencopySession {
    pub fn new(
        capture: &Arc<Capture>,
        screencopy_state: &ScreencopyState,
        qh: &QueueHandle<AppData>,
    ) -> Self {
        let udata = SessionData {
            session_data: Default::default(),
            capture: Arc::downgrade(capture),
        };

        let session = screencopy_state
            .capturer()
            .create_session(&capture.source, CaptureOptions::empty(), qh, udata)
            .unwrap();

        Self {
            buffers: None,
            session,
            release: None,
        }
    }

    fn attach_buffer_and_commit(
        &mut self,
        capture: &Arc<Capture>,
        conn: &Connection,
        qh: &QueueHandle<AppData>,
    ) {
        let Some(back) = self.buffers.as_ref().map(|x| &x[1]) else {
            return;
        };

        // TODO
        // let node = back.node().and_then(|x| x.to_str().map(|x| x.to_string()));

        self.session.capture(
            &back.buffer,
            &[],
            qh,
            FrameData {
                frame_data: Default::default(),
                capture: Arc::downgrade(&capture),
            },
        );
        conn.flush().unwrap();
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

struct FrameData {
    frame_data: ScreencopyFrameData,
    capture: Weak<Capture>,
}

impl ScreencopyFrameDataExt for FrameData {
    fn screencopy_frame_data(&self) -> &ScreencopyFrameData {
        &self.frame_data
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
        session: &CaptureSession,
        formats: &Formats,
    ) {
        let Some(capture) = Capture::for_session(session) else {
            return;
        };
        let mut session = capture.session.lock().unwrap();
        let Some(session) = session.as_mut() else {
            return;
        };

        // Create new buffer if none
        // XXX What if formats have changed?
        if session.buffers.is_none() {
            session.buffers = Some(array::from_fn(|_| self.create_buffer(formats)));
        }

        session.attach_buffer_and_commit(&capture, conn, &self.qh);
    }

    fn ready(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        capture_frame: &CaptureFrame,
        frame: Frame,
    ) {
        let capture = &capture_frame.data::<FrameData>().unwrap().capture;
        let Some(capture) = capture.upgrade() else {
            return;
        };
        let mut session = capture.session.lock().unwrap();
        let Some(session) = session.as_mut() else {
            return;
        };

        if session.buffers.is_none() {
            log::error!("No capture buffers?");
            return;
        }

        // swap buffers
        session.buffers.as_mut().unwrap().rotate_left(1);

        // Capture again on damage
        let capture_clone = capture.clone();
        let conn = conn.clone();
        let release = session.release.take();
        let qh = qh.clone();
        self.scheduler
            .schedule(async move {
                if let Some(release) = release {
                    // Wait for buffer to be released by server
                    release.await;
                }
                let mut session = capture_clone.session.lock().unwrap();
                let Some(session) = session.as_mut() else {
                    return;
                };
                session.attach_buffer_and_commit(&capture_clone, &conn, &qh);
            })
            .unwrap();

        let front = session.buffers.as_mut().unwrap().first_mut().unwrap();
        let (buffer, release) = SubsurfaceBuffer::new(front.backing.clone());
        session.release = Some(release);
        let image = CaptureImage {
            wl_buffer: buffer,
            width: front.size.0,
            height: front.size.1,
            transform: match frame.transform {
                WEnum::Value(value) => value,
                WEnum::Unknown(value) => panic!("invalid capture transform: {}", value),
            },
            #[cfg(feature = "no-subsurfaces")]
            image: cosmic::widget::image::Handle::from_rgba(
                front.size.0,
                front.size.1,
                front.mmap.to_vec(),
            ),
        };
        match &capture.source {
            CaptureSource::CosmicToplevel(toplevel) => {
                self.send_event(Event::ToplevelCapture(toplevel.clone(), image))
            }
            CaptureSource::CosmicWorkspace(workspace) => {
                self.send_event(Event::WorkspaceCapture(workspace.clone(), image));
            }
            CaptureSource::Output(_) | CaptureSource::Toplevel(_) => {
                unreachable!()
            }
        };
    }

    fn failed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        capture_frame: &CaptureFrame,
        reason: WEnum<FailureReason>,
    ) {
        // TODO
        log::error!("Screencopy failed: {:?}", reason);
        let capture = &capture_frame.data::<FrameData>().unwrap().capture;
        if let Some(capture) = capture.upgrade() {
            capture.stop();
        }
    }

    fn stopped(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, session: &CaptureSession) {
        // TODO
        if let Some(capture) = Capture::for_session(session) {
            capture.stop();
        }
    }
}

cctk::delegate_screencopy!(AppData, session: [SessionData], frame: [FrameData]);
