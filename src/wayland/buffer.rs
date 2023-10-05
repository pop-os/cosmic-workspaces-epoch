use cctk::{
    cosmic_protocols::screencopy::v1::client::zcosmic_screencopy_session_v1::BufferType,
    screencopy::BufferInfo,
    sctk::{
        dmabuf::{DmabufFeedback, DmabufFormat, DmabufState},
        globals::ProvidesBoundGlobal,
        shm::raw::RawPool,
    },
    wayland_client::{
        protocol::{wl_buffer, wl_shm},
        Connection, Dispatch, QueueHandle, WEnum,
    },
};
use cosmic::iced::widget::image;
use std::os::fd::AsFd;
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1;

use super::AppData;

pub struct Buffer {
    // pub pool: RawPool,
    pub buffer: wl_buffer::WlBuffer,
    pub buffer_info: BufferInfo,
}

// XXX need gbm device. I guess feedback is unneeded if we know what screncopy accepts? But need same modifiers as
// scanout.
// - use default feedback; get modifier for format. What modifier is best? I guess take first one.
impl AppData {
    fn create_gbm_buffer(&self, buffer_info: &BufferInfo) -> Option<gbm::BufferObject<()>> {
        let feedback = self.dmabuf_feedback.as_ref()?;
        let formats = feedback.format_table();
        let format_info = feedback
            .tranches()
            .iter()
            .flat_map(|x| &x.formats)
            .filter_map(|x| formats.get(*x as usize))
            .find(|x| x.format == buffer_info.format)?;
        let format = gbm::Format::try_from(buffer_info.format).ok()?;
        let modifier = gbm::Modifier::try_from(format_info.modifier).ok()?;
        self.gbm
            .create_buffer_object_with_modifiers(
                buffer_info.width,
                buffer_info.height,
                format,
                [modifier].into_iter(),
            )
            .ok()
    }

    pub fn create_buffer(&self, buffer_infos: &[BufferInfo]) -> Buffer {
        // XXX handle other formats?
        let format = wl_shm::Format::Abgr8888.into();

        if let Some(info) = buffer_infos
            .iter()
            .find(|x| x.type_ == WEnum::Value(BufferType::Dmabuf) && x.format == format)
        {
            if let Some(gbm_buffer) = self.create_gbm_buffer(info) {
                // XXX
                let fd = gbm_buffer.fd().unwrap();
                let stride = gbm_buffer.stride().unwrap();
                let modifier = gbm_buffer.modifier().unwrap();
                let mut params = self.dmabuf_state.create_params(&self.qh).unwrap();
                params.add(fd.as_fd(), 0, 0, stride, modifier.into());
                let buffer = params
                    .create_immed(
                        info.width as i32,
                        info.height as i32,
                        info.format,
                        zwp_linux_buffer_params_v1::Flags::empty(),
                        &self.qh,
                    )
                    .0;
                return Buffer {
                    buffer,
                    buffer_info: info.clone(),
                };
            }
        }

        // Fallback to shm buffer

        let buffer_info = buffer_infos
            .iter()
            .find(|x| x.type_ == WEnum::Value(BufferType::WlShm) && x.format == format)
            .unwrap();

        // Assume format is already known to be valid
        let mut pool = RawPool::new(
            (buffer_info.stride * buffer_info.height) as usize,
            &self.shm_state,
        )
        .unwrap();
        let format = wl_shm::Format::try_from(buffer_info.format).unwrap();
        let buffer = pool.create_buffer(
            0,
            buffer_info.width as i32,
            buffer_info.height as i32,
            buffer_info.stride as i32,
            format,
            (),
            &self.qh,
        );
        Buffer {
            buffer,
            buffer_info: buffer_info.clone(),
        }
    }

    /*
    // Buffer must be released by server for safety
    #[allow(clippy::wrong_self_convention)]
    pub unsafe fn to_image(&mut self) -> image::Handle {
        // XXX is this at all a performance issue?
        image::Handle::from_pixels(
            self.buffer_info.width,
            self.buffer_info.height,
            self.pool.mmap().to_vec(),
        )
    }
    */
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // XXX
        // self.buffer.destroy();
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for AppData {
    fn event(
        _app_data: &mut Self,
        _buffer: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_buffer::Event::Release => {}
            _ => unreachable!(),
        }
    }
}
