use cctk::{
    cosmic_protocols::screencopy::v1::client::zcosmic_screencopy_session_v1::BufferType,
    screencopy::BufferInfo,
    wayland_client::{
        protocol::{wl_buffer, wl_shm, wl_shm_pool},
        Connection, Dispatch, QueueHandle, WEnum,
    },
};
use cosmic::cctk;
use cosmic::iced_sctk::subsurface_widget::{BufferSource, Dmabuf, Plane, Shmbuf};
use rustix::{io::Errno, shm::ShmOFlags};
use std::{
    os::fd::{AsFd, OwnedFd},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1;

use super::AppData;

#[cfg(target_os = "linux")]
fn create_memfd() -> rustix::io::Result<OwnedFd> {
    let fd = rustix::io::retry_on_intr(|| {
        rustix::fs::memfd_create(
            "cosmic-workspaces-shm",
            rustix::fs::MemfdFlags::CLOEXEC | rustix::fs::MemfdFlags::ALLOW_SEALING,
        )
    })?;
    let _ = rustix::fs::fcntl_add_seals(
        &fd,
        rustix::fs::SealFlags::SHRINK | rustix::fs::SealFlags::SEAL,
    );
    Ok(fd)
}

fn create_memfile() -> rustix::io::Result<OwnedFd> {
    #[cfg(target_os = "linux")]
    if let Ok(fd) = create_memfd() {
        return Ok(fd);
    }

    loop {
        let flags = ShmOFlags::CREATE | ShmOFlags::EXCL | ShmOFlags::RDWR;

        let time = SystemTime::now();
        let name = format!(
            "/cosmic-workspaces-shm-{}",
            time.duration_since(UNIX_EPOCH).unwrap().subsec_nanos()
        );

        match rustix::shm::shm_open(&name, flags, 0600.into()) {
            Ok(fd) => match rustix::shm::shm_unlink(&name) {
                Ok(_) => return Ok(fd),
                Err(errno) => {
                    return Err(errno.into());
                }
            },
            #[allow(unreachable_patterns)]
            Err(Errno::EXIST | Errno::EXIST) => {
                continue;
            }
            Err(err) => return Err(err.into()),
        }
    }
}

pub struct Buffer {
    pub backing: Arc<BufferSource>,
    pub buffer: wl_buffer::WlBuffer,
    pub buffer_info: BufferInfo,
    node: Option<PathBuf>,
}

impl AppData {
    fn create_shm_buffer(&self, buffer_info: &BufferInfo) -> Buffer {
        let fd = create_memfile().unwrap(); // XXX?
        rustix::fs::ftruncate(&fd, buffer_info.stride as u64 * buffer_info.height as u64).unwrap();

        let pool = self.shm_state.wl_shm().create_pool(
            fd.as_fd(),
            buffer_info.stride as i32 * buffer_info.height as i32,
            &self.qh,
            (),
        );

        pool.destroy();

        // XXX
        let fd = rustix::fs::memfd_create("shm-buffer", rustix::fs::MemfdFlags::CLOEXEC).unwrap();
        rustix::fs::ftruncate(&fd, buffer_info.stride as u64 * buffer_info.height as u64).unwrap();

        let format = wl_shm::Format::try_from(buffer_info.format).unwrap();
        let buffer = pool.create_buffer(
            0,
            buffer_info.width as i32,
            buffer_info.height as i32,
            buffer_info.stride as i32,
            format,
            &self.qh,
            (),
        );

        Buffer {
            backing: Arc::new(
                Shmbuf {
                    fd,
                    offset: 0,
                    width: buffer_info.width as i32,
                    height: buffer_info.height as i32,
                    stride: buffer_info.stride as i32,
                    format,
                }
                .into(),
            ),
            buffer,
            buffer_info: buffer_info.clone(),
            node: None,
        }
    }

    #[allow(dead_code)]
    fn create_gbm_buffer(
        &self,
        buffer_info: &BufferInfo,
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
                x.format == buffer_info.format
                    && (!needs_linear || x.modifier == u64::from(gbm::Modifier::Linear))
            })
            .filter_map(|x| gbm::Modifier::try_from(x.modifier).ok())
            .collect::<Vec<_>>();

        if modifiers.is_empty() {
            return Ok(None);
        };
        let format = gbm::Format::try_from(buffer_info.format)?;
        //dbg!(format, modifiers);
        let bo = if !modifiers.iter().all(|x| *x == gbm::Modifier::Invalid) {
            gbm.create_buffer_object_with_modifiers::<()>(
                buffer_info.width,
                buffer_info.height,
                format,
                modifiers.iter().copied(),
            )?
        } else {
            // TODO make sure this isn't used across different GPUs
            gbm.create_buffer_object::<()>(
                buffer_info.width,
                buffer_info.height,
                format,
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
                buffer_info.width as i32,
                buffer_info.height as i32,
                buffer_info.format,
                zwp_linux_buffer_params_v1::Flags::empty(),
                &self.qh,
            )
            .0;

        Ok(Some(Buffer {
            backing: Arc::new(
                Dmabuf {
                    width: buffer_info.width as i32,
                    height: buffer_info.height as i32,
                    planes,
                    format: buffer_info.format,
                    modifier: modifier.into(),
                }
                .into(),
            ),
            buffer,
            buffer_info: buffer_info.clone(),
            node: Some(node.clone()),
        }))
    }

    pub fn create_buffer(&self, buffer_infos: &[BufferInfo]) -> Buffer {
        // XXX Handle other formats?
        let format = wl_shm::Format::Abgr8888.into();

        if let Some(buffer_info) = buffer_infos
            .iter()
            .find(|x| x.type_ == WEnum::Value(BufferType::Dmabuf) && x.format == format)
        {
            match self.create_gbm_buffer(buffer_info, false) {
                Ok(Some(buffer)) => {
                    return buffer;
                }
                Ok(None) => {}
                Err(err) => eprintln!("Failed to create gbm buffer: {}", err),
            }
        }

        // Fallback to shm buffer
        // Assume format is already known to be valid
        let buffer_info = buffer_infos
            .iter()
            .find(|x| x.type_ == WEnum::Value(BufferType::WlShm) && x.format == format)
            .unwrap();
        self.create_shm_buffer(buffer_info)
    }
}

impl Buffer {
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
