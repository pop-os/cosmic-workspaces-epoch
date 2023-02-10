use cctk::{
    wayland_client::Proxy,
    workspace::{WorkspaceHandler, WorkspaceState},
};

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
                if let Some(output) = group.output.as_ref() {
                    if let Some(output_name) = self.output_names.get(&output.id()).unwrap().clone()
                    {
                        workspaces.push((output_name, workspace.clone()));

                        self.add_capture_source(CaptureSource::Workspace(
                            workspace.handle.clone(),
                            output.clone(),
                        ));
                    }
                }
            }
        }

        self.send_event(Event::Workspaces(workspaces));
    }
}

cctk::delegate_workspace!(AppData);
