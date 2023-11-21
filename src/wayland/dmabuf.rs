use cctk::{
    sctk::{
        self,
        dmabuf::{DmabufFeedback, DmabufHandler, DmabufState},
    },
    wayland_client::{protocol::wl_buffer, Connection, QueueHandle},
};
use cosmic::cctk;

use std::{fs, io, os::unix::fs::MetadataExt, path::PathBuf};

use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
    zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
};

use super::AppData;

impl DmabufHandler for AppData {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }
    fn dmabuf_feedback(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _proxy: &ZwpLinuxDmabufFeedbackV1,
        feedback: DmabufFeedback,
    ) {
        if self.gbm.is_none() {
            #[allow(clippy::unnecessary_cast)]
            match find_gbm_device(feedback.main_device() as u64) {
                Ok(Some(gbm)) => {
                    self.gbm = Some(gbm);
                }
                Ok(None) => {
                    eprintln!("Gbm main device '{}' not found", feedback.main_device());
                }
                Err(err) => {
                    eprintln!("Failed to open gbm main device: {}", err);
                }
            }
        }
        self.dmabuf_feedback = Some(feedback);
    }
    fn created(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _params: &ZwpLinuxBufferParamsV1,
        _buffer: wl_buffer::WlBuffer,
    ) {
    }
    fn failed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _params: &ZwpLinuxBufferParamsV1,
    ) {
    }
    fn released(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _buffer: &wl_buffer::WlBuffer,
    ) {
    }
}

fn find_gbm_device(dev: u64) -> io::Result<Option<(PathBuf, gbm::Device<fs::File>)>> {
    for i in std::fs::read_dir("/dev/dri")? {
        let i = i?;
        if i.metadata()?.rdev() == dev {
            let file = fs::File::options().read(true).write(true).open(i.path())?;
            eprintln!("Opened gbm main device '{}'", i.path().display());
            return Ok(Some((i.path(), gbm::Device::new(file)?)));
        }
    }
    Ok(None)
}

sctk::delegate_dmabuf!(AppData);
