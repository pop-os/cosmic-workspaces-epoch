use cctk::{
    screencopy::Formats,
    wayland_client::{
        protocol::{wl_buffer, wl_shm, wl_shm_pool},
        Connection, Dispatch, QueueHandle,
    },
};
use cosmic::{
    cctk,
    iced_winit::platform_specific::wayland::subsurface_widget::{
        BufferSource, Dmabuf, Plane, Shmbuf,
    },
};
use std::{
    os::fd::AsFd,
    path::{Path, PathBuf},
    sync::Arc,
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1;

use super::AppData;
use crate::utils;

pub struct Buffer {
    pub backing: Arc<BufferSource>,
    pub buffer: wl_buffer::WlBuffer,
    node: Option<PathBuf>,
    pub size: (u32, u32),
    #[cfg(feature = "no-subsurfaces")]
    pub mmap: memmap2::Mmap,
}

impl AppData {
    fn create_shm_buffer(&self, format: u32, (width, height): (u32, u32)) -> Buffer {
        let fd = utils::create_memfile().unwrap(); // XXX?
        rustix::fs::ftruncate(&fd, width as u64 * height as u64 * 4).unwrap();

        let pool = self.shm_state.wl_shm().create_pool(
            fd.as_fd(),
            width as i32 * height as i32 * 4,
            &self.qh,
            (),
        );

        let format = wl_shm::Format::try_from(format).unwrap();
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            width as i32 * 4,
            format,
            &self.qh,
            (),
        );

        pool.destroy();

        #[cfg(feature = "no-subsurfaces")]
        let mmap = unsafe { memmap2::Mmap::map(&fd).unwrap() };

        Buffer {
            backing: Arc::new(
                Shmbuf {
                    fd,
                    offset: 0,
                    width: width as i32,
                    height: height as i32,
                    stride: width as i32 * 4,
                    format,
                }
                .into(),
            ),
            buffer,
            #[cfg(feature = "no-subsurfaces")]
            mmap,
            node: None,
            size: (width, height),
        }
    }

    #[cfg(not(feature = "force-shm-screencopy"))]
    fn create_gbm_buffer(
        &self,
        format: u32,
        modifiers: &[u64],
        (width, height): (u32, u32),
        needs_linear: bool,
    ) -> anyhow::Result<Option<Buffer>> {
        let (Some((node, gbm)), Some(feedback)) =
            (self.gbm.as_ref(), self.dmabuf_feedback.as_ref())
        else {
            return Ok(None);
        };
        let formats = feedback.format_table();

        let modifiers = feedback
            .tranches()
            .iter()
            .flat_map(|x| &x.formats)
            .filter_map(|x| formats.get(*x as usize))
            .filter(|x| {
                x.format == format
                    && (!needs_linear || x.modifier == u64::from(gbm::Modifier::Linear))
            })
            .map(|x| gbm::Modifier::from(x.modifier))
            .filter(|x| modifiers.contains(&u64::from(*x)))
            .collect::<Vec<_>>();

        if modifiers.is_empty() {
            return Ok(None);
        };
        let gbm_format = gbm::Format::try_from(format)?;
        //dbg!(format, modifiers);
        let bo = if !modifiers.iter().all(|x| *x == gbm::Modifier::Invalid) {
            gbm.create_buffer_object_with_modifiers::<()>(
                width,
                height,
                gbm_format,
                modifiers.iter().copied(),
            )?
        } else {
            // TODO make sure this isn't used across different GPUs
            gbm.create_buffer_object::<()>(
                width,
                height,
                gbm_format,
                gbm::BufferObjectFlags::empty(),
            )?
        };

        let mut planes = Vec::new();

        let params = self.dmabuf_state.create_params(&self.qh)?;
        let modifier = bo.modifier()?;
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
            planes.push(Plane {
                fd: plane_fd,
                plane_idx: i as u32,
                offset: plane_offset,
                stride: plane_stride,
            });
        }
        let buffer = params
            .create_immed(
                width as i32,
                height as i32,
                format,
                zwp_linux_buffer_params_v1::Flags::empty(),
                &self.qh,
            )
            .0;

        Ok(Some(Buffer {
            backing: Arc::new(
                Dmabuf {
                    width: width as i32,
                    height: height as i32,
                    planes,
                    format,
                    modifier: modifier.into(),
                }
                .into(),
            ),
            buffer,
            node: Some(node.clone()),
            size: (width, height),
        }))
    }

    pub fn create_buffer(&self, formats: &Formats) -> Buffer {
        // XXX Handle other formats?
        let format = u32::from(wl_shm::Format::Abgr8888);

        #[cfg(not(feature = "force-shm-screencopy"))]
        if let Some((_, modifiers)) = formats.dmabuf_formats.iter().find(|(f, _)| *f == format) {
            match self.create_gbm_buffer(format, &modifiers, formats.buffer_size, false) {
                Ok(Some(buffer)) => {
                    return buffer;
                }
                Ok(None) => {}
                Err(err) => log::error!("Failed to create gbm buffer: {}", err),
            }
        }

        // Fallback to shm buffer
        // Assume format is already known to be valid
        assert!(formats.shm_formats.contains(&format));
        self.create_shm_buffer(format, formats.buffer_size)
    }
}

impl Buffer {
    // Use this when dmabuf/screencopy has a way to specify node
    #[allow(dead_code)]
    pub fn node(&self) -> Option<&Path> {
        self.node.as_deref()
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

impl Dispatch<wl_shm_pool::WlShmPool, ()> for AppData {
    fn event(
        _app_data: &mut Self,
        _shm: &wl_shm_pool::WlShmPool,
        _event: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}
