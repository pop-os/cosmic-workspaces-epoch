//! Types related to drag-and-drop

use cosmic::{
    cctk::wayland_client::{protocol::wl_output, Proxy},
    iced::clipboard::mime::AsMimeTypes,
};
use std::{borrow::Cow, sync::LazyLock};

use crate::backend::{ExtForeignToplevelHandleV1, ExtWorkspaceHandleV1};

// Include `pid` in mime. Want to drag between our surfaces, but not another
// process, if we use Wayland object ids.
static WORKSPACE_MIME: LazyLock<String> =
    LazyLock::new(|| format!("text/x.cosmic-workspace-id-{}", std::process::id()));

static TOPLEVEL_MIME: LazyLock<String> =
    LazyLock::new(|| format!("text/x.cosmic-toplevel-id-{}", std::process::id()));

#[derive(Clone, Debug)]
pub enum DragSurface {
    #[allow(dead_code)]
    Workspace(ExtWorkspaceHandleV1),
    Toplevel(ExtForeignToplevelHandleV1),
}

// TODO store protocol object id?
#[derive(Clone, Debug)]
pub struct DragToplevel {}

impl AsMimeTypes for DragToplevel {
    fn available(&self) -> Cow<'static, [String]> {
        vec![TOPLEVEL_MIME.clone()].into()
    }

    fn as_bytes(&self, mime_type: &str) -> Option<Cow<'static, [u8]>> {
        if mime_type == *TOPLEVEL_MIME {
            Some(Vec::new().into())
        } else {
            None
        }
    }
}

impl cosmic::iced::clipboard::mime::AllowedMimeTypes for DragToplevel {
    fn allowed() -> Cow<'static, [String]> {
        vec![TOPLEVEL_MIME.clone()].into()
    }
}

impl TryFrom<(Vec<u8>, std::string::String)> for DragToplevel {
    type Error = ();
    fn try_from((_bytes, mime_type): (Vec<u8>, String)) -> Result<Self, ()> {
        if mime_type == *TOPLEVEL_MIME {
            Ok(Self {})
        } else {
            Err(())
        }
    }
}

#[derive(Clone, Debug)]
pub struct DragWorkspace {}

impl AsMimeTypes for DragWorkspace {
    fn available(&self) -> Cow<'static, [String]> {
        vec![WORKSPACE_MIME.clone()].into()
    }

    fn as_bytes(&self, mime_type: &str) -> Option<Cow<'static, [u8]>> {
        if mime_type == *WORKSPACE_MIME {
            Some(Vec::new().into())
        } else {
            None
        }
    }
}

impl cosmic::iced::clipboard::mime::AllowedMimeTypes for DragWorkspace {
    fn allowed() -> Cow<'static, [String]> {
        vec![WORKSPACE_MIME.clone()].into()
    }
}

impl TryFrom<(Vec<u8>, std::string::String)> for DragWorkspace {
    type Error = ();
    fn try_from((_bytes, mime_type): (Vec<u8>, String)) -> Result<Self, ()> {
        if mime_type == *WORKSPACE_MIME {
            Ok(Self {})
        } else {
            Err(())
        }
    }
}

// TODO name?
pub enum Drag {
    Toplevel,
    Workspace,
}

impl cosmic::iced::clipboard::mime::AllowedMimeTypes for Drag {
    fn allowed() -> Cow<'static, [String]> {
        vec![TOPLEVEL_MIME.clone(), WORKSPACE_MIME.clone()].into()
    }
}

impl TryFrom<(Vec<u8>, std::string::String)> for Drag {
    type Error = ();
    fn try_from((_bytes, mime_type): (Vec<u8>, String)) -> Result<Self, ()> {
        if mime_type == *TOPLEVEL_MIME {
            Ok(Self::Toplevel)
        } else if mime_type == *WORKSPACE_MIME {
            Ok(Self::Workspace)
        } else {
            Err(())
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum DropTarget {
    WorkspaceSidebarEntry(ExtWorkspaceHandleV1, wl_output::WlOutput),
    OutputToplevels(ExtWorkspaceHandleV1, wl_output::WlOutput),
    #[allow(dead_code)]
    WorkspacesBar(wl_output::WlOutput),
}

impl DropTarget {
    /// Encode as a u64 for iced/smithay_sctk to associate drag destination area with widget.
    pub fn drag_id(&self) -> u64 {
        // https://doc.rust-lang.org/std/mem/fn.discriminant.html#accessing-the-numeric-value-of-the-discriminant
        let discriminant = unsafe { *<*const _>::from(self).cast::<u8>() };
        match self {
            Self::WorkspaceSidebarEntry(workspace, _output) => {
                // TODO consider workspace that span multiple outputs?
                let id = workspace.id().protocol_id();
                (u64::from(discriminant) << 32) | u64::from(id)
            }
            Self::OutputToplevels(_workspace, output) => {
                let id = output.id().protocol_id();
                (u64::from(discriminant) << 32) | u64::from(id)
            }
            Self::WorkspacesBar(output) => {
                let id = output.id().protocol_id();
                (u64::from(discriminant) << 32) | u64::from(id)
            }
        }
    }
}
