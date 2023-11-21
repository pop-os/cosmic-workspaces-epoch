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
            for workspace in &group.workspaces {
                workspaces.push((group.outputs.iter().cloned().collect(), workspace.clone()));

                for output in &group.outputs {
                    self.add_capture_source(CaptureSource::Workspace(
                        workspace.handle.clone(),
                        output.clone(),
                    ));
                }
            }
        }

        self.send_event(Event::Workspaces(workspaces));
    }
}

cctk::delegate_workspace!(AppData);
