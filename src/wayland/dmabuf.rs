use cctk::{
    sctk::{
        self,
        dmabuf::{DmabufFeedback, DmabufHandler, DmabufState},
    },
    wayland_client::{
        globals::registry_queue_init,
        protocol::{
            wl_buffer, wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_subsurface,
            wl_surface,
        },
        Connection, Dispatch, Proxy, QueueHandle, WEnum,
    },
};
use std::{convert::TryInto, time::Duration};
use std::{
    fs,
    os::{
        fd::AsFd,
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    str,
    sync::{Arc, Mutex},
};

use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1},
    zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
};

use super::AppData;

impl DmabufHandler for AppData {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }
    fn dmabuf_feedback(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        proxy: &ZwpLinuxDmabufFeedbackV1,
        feedback: DmabufFeedback,
    ) {
    }
    fn created(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        params: &ZwpLinuxBufferParamsV1,
        buffer: wl_buffer::WlBuffer,
    ) {
    }
    fn failed(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        params: &ZwpLinuxBufferParamsV1,
    ) {
    }
    fn released(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        buffer: &wl_buffer::WlBuffer,
    ) {
    }
}

sctk::delegate_dmabuf!(AppData);
