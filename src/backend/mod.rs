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
pub use cosmic::cctk::{toplevel_info::ToplevelInfo, workspace::Workspace};
#[cfg(not(feature = "mock-backend"))]
pub use wayland_protocols::ext::{
    foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
    workspace::v1::client::ext_workspace_handle_v1::ExtWorkspaceHandleV1,
};

#[cfg(not(feature = "mock-backend"))]
pub use wayland::subscription;

// Mock backend
#[cfg(feature = "mock-backend")]
mod mock;
#[cfg(feature = "mock-backend")]
pub use mock::{
    subscription, ExtForeignToplevelHandleV1, ExtWorkspaceHandleV1, ToplevelInfo, Workspace,
};

#[derive(Clone, Debug, Default)]
pub struct CaptureFilter {
    pub workspaces_on_outputs: Vec<wl_output::WlOutput>,
    pub toplevels_on_workspaces: Vec<ExtWorkspaceHandleV1>,
}

#[derive(Clone, Debug)]
pub struct CaptureImage {
    #[allow(dead_code)]
    pub width: u32,
    #[allow(dead_code)]
    pub height: u32,
    pub wl_buffer: SubsurfaceBuffer,
    pub transform: wl_output::Transform,
    #[cfg(feature = "no-subsurfaces")]
    pub image: cosmic::widget::image::Handle,
}

#[derive(Clone, Debug)]
pub enum Event {
    CmdSender(calloop::channel::Sender<Cmd>),
    Workspaces(Vec<(HashSet<wl_output::WlOutput>, Workspace)>),
    WorkspaceCapture(ExtWorkspaceHandleV1, CaptureImage),
    NewToplevel(ExtForeignToplevelHandleV1, ToplevelInfo),
    UpdateToplevel(ExtForeignToplevelHandleV1, ToplevelInfo),
    CloseToplevel(ExtForeignToplevelHandleV1),
    ToplevelCapture(ExtForeignToplevelHandleV1, CaptureImage),
}

#[derive(Debug)]
pub enum Cmd {
    CaptureFilter(CaptureFilter),
    ActivateToplevel(ExtForeignToplevelHandleV1),
    CloseToplevel(ExtForeignToplevelHandleV1),
    MoveToplevelToWorkspace(
        ExtForeignToplevelHandleV1,
        ExtWorkspaceHandleV1,
        wl_output::WlOutput,
    ),
    MoveWorkspaceBefore(ExtWorkspaceHandleV1, ExtWorkspaceHandleV1),
    MoveWorkspaceAfter(ExtWorkspaceHandleV1, ExtWorkspaceHandleV1),
    ActivateWorkspace(ExtWorkspaceHandleV1),
    SetWorkspacePinned(ExtWorkspaceHandleV1, bool),
}
