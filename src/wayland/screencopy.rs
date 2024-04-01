use cosmic::cctk::{
    self,
    cosmic_protocols::{
        image_source::v1::client::{
            zcosmic_toplevel_image_source_manager_v1::ZcosmicToplevelImageSourceManagerV1,
            zcosmic_workspace_image_source_manager_v1::ZcosmicWorkspaceImageSourceManagerV1,
        },
        screencopy::v2::client::{
            zcosmic_screencopy_frame_v2, zcosmic_screencopy_manager_v2,
            zcosmic_screencopy_session_v2,
        },
    },
    screencopy::{
        capture, Formats, Frame, ScreencopyFrameData, ScreencopyFrameDataExt, ScreencopyHandler,
        ScreencopySessionData, ScreencopySessionDataExt, ScreencopyState,
    },
    wayland_client::{Connection, Proxy, QueueHandle, WEnum},
};
use cosmic::iced_sctk::subsurface_widget::{SubsurfaceBuffer, SubsurfaceBufferRelease};
use std::{
    array,
    sync::{Arc, Weak},
};

use super::{AppData, Buffer, Capture, CaptureImage, CaptureSource, Event};

pub struct ScreencopySession {
    // swapchain buffers
    buffers: Option<[Buffer; 2]>,
    session: zcosmic_screencopy_session_v2::ZcosmicScreencopySessionV2,
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
        let image_source = match &capture.source {
            CaptureSource::Toplevel(toplevel) => screencopy_state
                .toplevel_source_manager
                .as_ref()
                .unwrap()
                .create_source(toplevel, qh, ()),
            CaptureSource::Workspace(workspace, output) => screencopy_state
                .workspace_source_manager
                .as_ref()
                .unwrap()
                .create_source(
                    workspace,
                    // output,
                    qh,
                    (),
                ),
        };

        let udata = SessionData {
            session_data: Default::default(),
            capture: Arc::downgrade(capture),
        };

        let session = screencopy_state.screencopy_manager.create_session(
            &image_source,
            zcosmic_screencopy_manager_v2::Options::empty(),
            qh,
            udata,
        );

        Self {
            buffers: None,
            session,
            release: None,
        }
    }

    fn attach_buffer_and_commit(
        &mut self,
        _capture: &Capture,
        conn: &Connection,
        qh: &QueueHandle<AppData>,
    ) {
        let Some(back) = self.buffers.as_ref().map(|x| &x[1]) else {
            return;
        };

        // TODO
        // let node = back.node().and_then(|x| x.to_str().map(|x| x.to_string()));

        capture(
            &self.session,
            &back.buffer,
            &[],
            qh,
            FrameData {
                frame_data: Default::default(),
                session: self.session.clone(),
            },
        );
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

struct FrameData {
    frame_data: ScreencopyFrameData,
    session: zcosmic_screencopy_session_v2::ZcosmicScreencopySessionV2,
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
        session: &zcosmic_screencopy_session_v2::ZcosmicScreencopySessionV2,
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
        screencopy_frame: &zcosmic_screencopy_frame_v2::ZcosmicScreencopyFrameV2,
        frame: Frame,
    ) {
        let session = &screencopy_frame.data::<FrameData>().unwrap().session;
        let Some(capture) = Capture::for_session(session) else {
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
            #[cfg(feature = "no-subsurfaces")]
            image: cosmic::widget::image::Handle::from_pixels(
                front.size.0,
                front.size.1,
                front.mmap.to_vec(),
            ),
        };
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
    }

    fn failed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        screencopy_frame: &zcosmic_screencopy_frame_v2::ZcosmicScreencopyFrameV2,
        reason: WEnum<zcosmic_screencopy_frame_v2::FailureReason>,
    ) {
        // TODO
        log::error!("Screencopy failed: {:?}", reason);
        let session = &screencopy_frame.data::<FrameData>().unwrap().session;
        if let Some(capture) = Capture::for_session(session) {
            capture.stop();
        }
    }

    fn stopped(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        session: &zcosmic_screencopy_session_v2::ZcosmicScreencopySessionV2,
    ) {
        // TODO
        if let Some(capture) = Capture::for_session(session) {
            capture.stop();
        }
    }
}

cctk::delegate_screencopy!(AppData, session: [SessionData], frame: [FrameData]);
