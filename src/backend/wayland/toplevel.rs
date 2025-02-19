use cctk::{
    cosmic_protocols::toplevel_management::v1::client::zcosmic_toplevel_manager_v1,
    toplevel_info::{ToplevelInfoHandler, ToplevelInfoState},
    toplevel_management::{ToplevelManagerHandler, ToplevelManagerState},
    wayland_client::{Connection, QueueHandle, WEnum},
};
use cosmic::cctk;
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1;

use super::{AppData, CaptureSource, Event};

// TODO any indication when we have all toplevels?
impl ToplevelInfoHandler for AppData {
    fn toplevel_info_state(&mut self) -> &mut ToplevelInfoState {
        &mut self.toplevel_info_state
    }

    fn new_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        let info = self.toplevel_info_state.info(toplevel).unwrap();
        let cosmic_toplevel = info.cosmic_toplevel.clone().unwrap();
        self.send_event(Event::NewToplevel(toplevel.clone(), info.clone()));

        self.add_capture_source(CaptureSource::CosmicToplevel(cosmic_toplevel));
    }

    fn update_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        let info = self.toplevel_info_state.info(toplevel).unwrap();
        self.send_event(Event::UpdateToplevel(toplevel.clone(), info.clone()));
    }

    fn toplevel_closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        let info = self.toplevel_info_state.info(toplevel).unwrap();
        let cosmic_toplevel = info.cosmic_toplevel.clone().unwrap();
        self.send_event(Event::CloseToplevel(toplevel.clone()));

        self.remove_capture_source(CaptureSource::CosmicToplevel(cosmic_toplevel));
    }
}

impl ToplevelManagerHandler for AppData {
    fn toplevel_manager_state(&mut self) -> &mut ToplevelManagerState {
        &mut self.toplevel_manager_state
    }

    fn capabilities(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _capabilities: Vec<
            WEnum<zcosmic_toplevel_manager_v1::ZcosmicToplelevelManagementCapabilitiesV1>,
        >,
    ) {
    }
}

cctk::delegate_toplevel_info!(AppData);
cctk::delegate_toplevel_manager!(AppData);
