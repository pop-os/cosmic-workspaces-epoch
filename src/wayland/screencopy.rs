use cctk::{
    cosmic_protocols::screencopy::v1::client::zcosmic_screencopy_session_v1,
    screencopy::{BufferInfo, ScreencopyHandler, ScreencopyState},
    wayland_client::{Connection, QueueHandle, WEnum},
};
use cosmic::cctk;

use super::{AppData, Capture, CaptureImage, CaptureSource, Event};

fn attach_buffer_and_commit(capture: &Capture, conn: &Connection) {
    let session = capture.session.lock().unwrap();
    let buffer = capture.buffer.lock().unwrap();
    let (Some(session), Some(buffer)) = (session.as_ref(), buffer.as_ref()) else {
        return;
    };

    let node = buffer
        .node()
        .and_then(|x| x.to_str().map(|x| x.to_string()));

    session.attach_buffer(&buffer.buffer, node, 0); // XXX age?
    if capture.first_frame() {
        session.commit(zcosmic_screencopy_session_v1::Options::empty());
        capture.unset_first_frame();
    } else {
        session.commit(zcosmic_screencopy_session_v1::Options::OnDamage);
    }
    conn.flush().unwrap();
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

        let mut buffer = capture.buffer.lock().unwrap();
        // Create new buffer if none, or different format
        if !buffer
            .as_ref()
            .map_or(false, |x| buffer_infos.contains(&x.buffer_info))
        {
            *buffer = Some(self.create_buffer(buffer_infos));
        }

        drop(buffer);

        attach_buffer_and_commit(&capture, conn);
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
        if !capture.running() {
            return;
        }

        let mut buffer = capture.buffer.lock().unwrap();
        if buffer.is_none() {
            eprintln!("Error: No capture buffer?");
            return;
        }
        let img = unsafe { buffer.as_mut().unwrap().to_image() };
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

        drop(buffer);

        // Capture again on damage
        attach_buffer_and_commit(&capture, conn);
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
