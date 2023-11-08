use cctk::{
    cosmic_protocols::screencopy::v1::client::zcosmic_screencopy_session_v1::BufferType,
    screencopy::BufferInfo,
    sctk::shm::raw::RawPool,
    wayland_client::{
        protocol::{wl_buffer, wl_shm},
        Connection, Dispatch, QueueHandle, WEnum,
    },
};
use cosmic::iced::widget::image;

use super::AppData;

pub struct Buffer {
    pub pool: RawPool,
    pub buffer: wl_buffer::WlBuffer,
    pub buffer_info: BufferInfo,
}

impl AppData {
    pub fn create_buffer(&self, buffer_infos: &[BufferInfo]) -> Buffer {
        // XXX Handle other formats?
        let format = wl_shm::Format::Abgr8888.into();

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
            pool,
            buffer,
            buffer_info: buffer_info.clone(),
        }
    }
}

impl Buffer {
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
}

impl Drop for Buffer {
    fn drop(&mut self) {
        self.buffer.destroy();
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
