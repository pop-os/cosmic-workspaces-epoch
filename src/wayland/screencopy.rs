use cctk::{
    cosmic_protocols::screencopy::v1::client::zcosmic_screencopy_session_v1,
    screencopy::{BufferInfo, ScreencopyHandler, ScreencopyState},
    wayland_client::{Connection, QueueHandle, WEnum},
};

use super::{AppData, Capture, CaptureImage, CaptureSource, Event};

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
        let capture = Capture::for_session(session).unwrap();
        if !capture.running() {
            session.destroy();
            return;
        }

        let mut buffer = capture.buffer.lock().unwrap();
        // Create new buffer if none, or different format
        if !buffer
            .as_ref()
            .map_or(false, |x| buffer_infos.contains(&x.buffer_info))
        {
            *buffer = Some(self.create_buffer(buffer_infos));
        }
        let buffer = buffer.as_ref().unwrap();

        let node = buffer
            .node()
            .and_then(|x| x.to_str().map(|x| x.to_string()));
        session.attach_buffer(&buffer.buffer, node, 0); // XXX age?
        if capture.first_frame() {
            session.commit(zcosmic_screencopy_session_v1::Options::empty());
        } else {
            session.commit(zcosmic_screencopy_session_v1::Options::OnDamage);
        }
        conn.flush().unwrap();
    }

    fn ready(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        session: &zcosmic_screencopy_session_v1::ZcosmicScreencopySessionV1,
    ) {
        let capture = Capture::for_session(session).unwrap();
        if !capture.running() {
            session.destroy();
            return;
        }

        let mut buffer = capture.buffer.lock().unwrap();
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
        session.destroy();

        // Capture again on damage
        capture.capture(&self.screencopy_state.screencopy_manager, &self.qh);
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
        let capture = Capture::for_session(session).unwrap();
        capture.cancel();
        session.destroy();
    }
}
