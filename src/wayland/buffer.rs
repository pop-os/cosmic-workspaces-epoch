use cctk::{
    cosmic_protocols::screencopy::v1::client::zcosmic_screencopy_session_v1::BufferType,
    screencopy::BufferInfo,
    sctk::shm::raw::RawPool,
    wayland_client::{
        protocol::{wl_buffer, wl_shm},
        Connection, Dispatch, QueueHandle, WEnum,
    },
};
use cosmic::cctk;
use cosmic::iced::widget::image;
use std::{
    os::fd::{AsFd, OwnedFd},
    path::{Path, PathBuf},
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1;

use super::AppData;

enum BufferBacking {
    Shm {
        pool: RawPool,
    },
    Dmabuf {
        fd: OwnedFd,
        node: PathBuf,
        stride: u32,
    },
}

pub struct Buffer {
    backing: BufferBacking,
    pub buffer: wl_buffer::WlBuffer,
    pub buffer_info: BufferInfo,
}

impl AppData {
    fn create_shm_backing(&self, buffer_info: &BufferInfo) -> (BufferBacking, wl_buffer::WlBuffer) {
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

        (BufferBacking::Shm { pool }, buffer)
    }

    #[allow(dead_code)]
    fn create_gbm_backing(
        &self,
        buffer_info: &BufferInfo,
        needs_linear: bool,
    ) -> anyhow::Result<Option<(BufferBacking, wl_buffer::WlBuffer)>> {
        let (Some((node, gbm)), Some(feedback)) =
            (self.gbm.as_ref(), self.dmabuf_feedback.as_ref())
        else {
            return Ok(None);
        };
        let formats = feedback.format_table();
        let Some(format_info) = feedback
            .tranches()
            .iter()
            .flat_map(|x| &x.formats)
            .filter_map(|x| formats.get(*x as usize))
            .find(|x| {
                x.format == buffer_info.format
                    && (!needs_linear || x.modifier == u64::from(gbm::Modifier::Linear))
            })
        else {
            return Ok(None);
        };
        let format = gbm::Format::try_from(buffer_info.format)?;
        let modifier = gbm::Modifier::try_from(format_info.modifier)?;
        let bo = gbm.create_buffer_object_with_modifiers::<()>(
            buffer_info.width,
            buffer_info.height,
            format,
            [modifier].into_iter(),
        )?;

        let fd = bo.fd()?;
        let stride = bo.stride()?;
        let params = self.dmabuf_state.create_params(&self.qh)?;
        for i in 0..bo.plane_count()? as i32 {
            let plane_fd = bo.fd_for_plane(i)?;
            let plane_offset = bo.offset(i)?;
            let plane_stride = bo.stride_for_plane(i)?;
            params.add(
                plane_fd.as_fd(),
                i as u32,
                plane_offset,
                plane_stride,
                modifier.into(),
            );
        }
        let buffer = params
            .create_immed(
                buffer_info.width as i32,
                buffer_info.height as i32,
                buffer_info.format,
                zwp_linux_buffer_params_v1::Flags::empty(),
                &self.qh,
            )
            .0;

        Ok(Some((
            BufferBacking::Dmabuf {
                fd,
                node: node.clone(),
                stride,
            },
            buffer,
        )))
    }

    pub fn create_buffer(&self, buffer_infos: &[BufferInfo]) -> Buffer {
        // XXX Handle other formats?
        let format = wl_shm::Format::Abgr8888.into();

        /*
        if let Some(buffer_info) = buffer_infos
            .iter()
            .find(|x| x.type_ == WEnum::Value(BufferType::Dmabuf) && x.format == format)
        {
            match self.create_gbm_backing(buffer_info, true) {
                Ok(Some((backing, buffer))) => {
                    return Buffer {
                        backing,
                        buffer,
                        buffer_info: buffer_info.clone(),
                    };
                }
                Ok(None) => {}
                Err(err) => eprintln!("Failed to create gbm buffer: {}", err),
            }
        }
        */

        // Fallback to shm buffer
        // Assume format is already known to be valid
        let buffer_info = buffer_infos
            .iter()
            .find(|x| x.type_ == WEnum::Value(BufferType::WlShm) && x.format == format)
            .unwrap();
        let (backing, buffer) = self.create_shm_backing(buffer_info);
        Buffer {
            backing,
            buffer,
            buffer_info: buffer_info.clone(),
        }
    }
}

impl Buffer {
    // Buffer must be released by server for safety
    // XXX is this at all a performance issue?
    #[allow(clippy::wrong_self_convention)]
    pub unsafe fn to_image(&mut self) -> image::Handle {
        let pixels = match &mut self.backing {
            BufferBacking::Shm { pool } => pool.mmap().to_vec(),
            // NOTE: Only will work with linear modifier
            BufferBacking::Dmabuf {
                fd,
                node: _,
                stride,
            } => {
                // XXX Error handling?
                let mmap = memmap2::Mmap::map(&*fd).unwrap();
                if self.buffer_info.stride == self.buffer_info.width * 4 {
                    mmap.to_vec()
                } else {
                    let width = self.buffer_info.width as usize;
                    let height = self.buffer_info.height as usize;
                    let stride = *stride as usize;
                    let output_stride = width * 4;
                    let mut pixels = vec![0; height * output_stride];
                    for y in 0..height {
                        pixels[y * output_stride..y * output_stride + output_stride]
                            .copy_from_slice(&mmap[y * stride..y * stride + output_stride]);
                    }
                    pixels
                }
            }
        };
        image::Handle::from_pixels(self.buffer_info.width, self.buffer_info.height, pixels)
    }

    pub fn node(&self) -> Option<&Path> {
        match &self.backing {
            BufferBacking::Shm { .. } => None,
            BufferBacking::Dmabuf { node, .. } => Some(node),
        }
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
