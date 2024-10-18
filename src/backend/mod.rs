// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

//! The backend of getting workspace/toplevel information and previews, and
//! sending commands to change them.
//!
//! There are two backends: one that uses cosmic-comp protocols, and a mock
//! backend for testing without any special protocols.

use cosmic::{
    cctk::wayland_client::protocol::wl_output,
    iced_winit::platform_specific::wayland::subsurface_widget::SubsurfaceBuffer,
};
use std::collections::HashSet;

// Wayland backend using cosmic-comp specific protocols
#[cfg(not(feature = "mock-backend"))]
mod wayland;
#[cfg(not(feature = "mock-backend"))]
pub use cosmic::cctk::{
    cosmic_protocols::{
        toplevel_info::v1::client::zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
        workspace::v1::client::zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1,
    },
    toplevel_info::ToplevelInfo,
    workspace::Workspace,
};
#[cfg(not(feature = "mock-backend"))]
pub use wayland::subscription;

// Mock backend
#[cfg(feature = "mock-backend")]
mod mock;
#[cfg(feature = "mock-backend")]
pub use mock::{
    subscription, ToplevelInfo, Workspace, ZcosmicToplevelHandleV1, ZcosmicWorkspaceHandleV1,
};

#[derive(Clone, Debug, Default)]
pub struct CaptureFilter {
    pub workspaces_on_outputs: Vec<wl_output::WlOutput>,
    pub toplevels_on_workspaces: Vec<ZcosmicWorkspaceHandleV1>,
}

#[derive(Clone, Debug)]
pub struct CaptureImage {
    pub width: u32,
    pub height: u32,
    pub wl_buffer: SubsurfaceBuffer,
    #[cfg(feature = "no-subsurfaces")]
    pub image: cosmic::widget::image::Handle,
}

#[derive(Clone, Debug)]
pub enum Event {
    CmdSender(calloop::channel::Sender<Cmd>),
    Workspaces(Vec<(HashSet<wl_output::WlOutput>, Workspace)>),
    WorkspaceCapture(ZcosmicWorkspaceHandleV1, wl_output::WlOutput, CaptureImage),
    NewToplevel(ZcosmicToplevelHandleV1, ToplevelInfo),
    UpdateToplevel(ZcosmicToplevelHandleV1, ToplevelInfo),
    CloseToplevel(ZcosmicToplevelHandleV1),
    ToplevelCapture(ZcosmicToplevelHandleV1, CaptureImage),
}

#[derive(Debug)]
pub enum Cmd {
    CaptureFilter(CaptureFilter),
    ActivateToplevel(ZcosmicToplevelHandleV1),
    CloseToplevel(ZcosmicToplevelHandleV1),
    MoveToplevelToWorkspace(
        ZcosmicToplevelHandleV1,
        ZcosmicWorkspaceHandleV1,
        wl_output::WlOutput,
    ),
    ActivateWorkspace(ZcosmicWorkspaceHandleV1),
}
