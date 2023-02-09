use cctk::{
    screencopy::BufferInfo,
    sctk::{globals::ProvidesBoundGlobal, shm::raw::RawPool},
    wayland_client::{
        protocol::{wl_buffer, wl_shm},
        QueueHandle,
    },
};
use cosmic::iced::widget::image;

pub struct Buffer {
    pub pool: RawPool,
    pub buffer: wl_buffer::WlBuffer,
    pub buffer_info: BufferInfo,
}

impl Buffer {
    pub fn new(
        buffer_info: BufferInfo,
        shm: &impl ProvidesBoundGlobal<wl_shm::WlShm, 1>,
        qh: &QueueHandle<super::AppData>,
    ) -> Self {
        // Assume format is already known to be valid
        let mut pool =
            RawPool::new((buffer_info.stride * buffer_info.height) as usize, shm).unwrap();
        let format = wl_shm::Format::try_from(buffer_info.format).unwrap();
        let buffer = pool.create_buffer(
            0,
            buffer_info.width as i32,
            buffer_info.height as i32,
            buffer_info.stride as i32,
            format,
            (),
            qh,
        );
        Self {
            pool,
            buffer,
            buffer_info,
        }
    }

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
