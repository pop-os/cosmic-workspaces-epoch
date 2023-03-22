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
                let output_names: Vec<_> = group
                    .outputs
                    .iter()
                    .filter_map(|output| self.output_names.get(&output.id()).cloned()?)
                    .collect();
                if !output_names.is_empty() {
                    workspaces.push((output_names, workspace.clone()));

                    for output in &group.outputs {
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
