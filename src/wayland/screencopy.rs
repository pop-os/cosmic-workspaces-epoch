use cctk::{
    cosmic_protocols::screencopy::v1::client::zcosmic_screencopy_session_v1,
    screencopy::{BufferInfo, ScreencopyHandler, ScreencopyState},
    wayland_client::{Connection, QueueHandle, WEnum},
};
use cosmic::cctk;

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

        capture.attach_buffer_and_commit(conn);
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
        capture.attach_buffer_and_commit(conn);
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
