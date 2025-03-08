use cctk::workspace::{WorkspaceHandler, WorkspaceState};
use cosmic::cctk;

use super::{AppData, CaptureSource, Event};

impl WorkspaceHandler for AppData {
    fn workspace_state(&mut self) -> &mut WorkspaceState {
        &mut self.workspace_state
    }

    fn done(&mut self) {
        let mut workspaces = Vec::new();

        // XXX remove capture source for removed workspaces
        // Handle move to another output

        for group in self.workspace_state.workspace_groups() {
            for workspace_handle in &group.workspaces {
                if let Some(workspace) = self.workspace_state.workspace_info(workspace_handle) {
                    workspaces.push((group.outputs.iter().cloned().collect(), workspace.clone()));

                    // TODO one capture per output on workspace?
                    self.add_capture_source(CaptureSource::Workspace(workspace_handle.clone()));
                }
            }
        }

        self.send_event(Event::Workspaces(workspaces));
    }
}

cctk::delegate_workspace!(AppData);
