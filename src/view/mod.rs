use cctk::{
    cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
    wayland_client::protocol::wl_output,
};
use cosmic::{
    cctk,
    iced::{
        self,
        widget::{column, row},
    },
    widget,
};
use cosmic_comp_config::workspace::{WorkspaceAmount, WorkspaceLayout};

use crate::{wayland::CaptureImage, App, DragSurface, LayerSurface, Msg, Toplevel, Workspace};

pub(crate) fn layer_surface<'a>(
    app: &'a App,
    surface: &'a LayerSurface,
) -> cosmic::Element<'a, Msg> {
    let layout = app.conf.workspace_config.workspace_layout;
    let sidebar = workspaces_sidebar(
        app.workspaces
            .iter()
            .filter(|i| i.outputs.contains(&surface.output)),
        &surface.output,
        layout,
        app.conf.workspace_config.workspace_amount,
    );
    let toplevels = toplevel_previews(
        app.toplevels.iter().filter(|i| {
            if !i.info.output.contains(&surface.output) {
                return false;
            }

            i.info.workspace.iter().any(|workspace| {
                app.workspace_for_handle(workspace)
                    .map_or(false, |x| x.is_active)
            })
        }),
        &surface.output,
    );
    match layout {
        WorkspaceLayout::Vertical => widget::cosmic_container::container(
            row![sidebar, toplevels]
                .spacing(12)
                .height(iced::Length::Fill)
                .width(iced::Length::Fill),
        ),
        WorkspaceLayout::Horizontal => widget::cosmic_container::container(
            column![sidebar, toplevels]
                .spacing(12)
                .height(iced::Length::Fill),
        ),
    }
    .into()
}

fn close_button(on_press: Msg) -> cosmic::Element<'static, Msg> {
    widget::button(widget::icon::from_name("window-close-symbolic").size(16))
        .style(cosmic::theme::Button::Destructive)
        .on_press(on_press)
        .into()
}

pub(crate) fn workspace_item<'a>(
    workspace: &'a Workspace,
    output: &wl_output::WlOutput,
) -> cosmic::Element<'a, Msg> {
    column![
        close_button(Msg::CloseWorkspace(workspace.handle.clone())),
        widget::button(column![
            capture_image(workspace.img_for_output.get(output)),
            widget::text(&workspace.name)
        ])
        .selected(workspace.is_active)
        .style(cosmic::theme::Button::Image)
        .on_press(Msg::ActivateWorkspace(workspace.handle.clone())),
    ]
    .height(iced::Length::Fill)
    .into()
}

fn workspace_sidebar_entry<'a>(
    workspace: &'a Workspace,
    output: &'a wl_output::WlOutput,
) -> cosmic::Element<'a, Msg> {
    /* TODO allow moving workspaces (needs compositor support)
    iced::widget::dnd_source(workspace_item(workspace, output))
        .on_drag(|size| {
            Msg::StartDrag(
                size,
                DragSurface::Workspace {
                    handle: workspace.handle.clone(),
                    output: output.clone(),
                },
            )
        })
        .on_finished(Msg::SourceFinished)
        .on_cancelled(Msg::SourceFinished)
        .into()
    */
    iced::widget::dnd_listener(workspace_item(workspace, output))
        .on_enter(|actions, mime, pos| {
            Msg::DndWorkspaceEnter(workspace.handle.clone(), output.clone(), actions, mime, pos)
        })
        .on_exit(Msg::DndWorkspaceLeave)
        .on_drop(Msg::DndWorkspaceDrop)
        .on_data(Msg::DndWorkspaceData)
        .into()
}

fn workspaces_sidebar<'a>(
    workspaces: impl Iterator<Item = &'a Workspace>,
    output: &'a wl_output::WlOutput,
    layout: WorkspaceLayout,
    amount: WorkspaceAmount,
) -> cosmic::Element<'a, Msg> {
    let mut sidebar_entries: Vec<_> = workspaces
        .map(|w| workspace_sidebar_entry(w, output))
        .collect();
    if amount != WorkspaceAmount::Dynamic {
        // TODO implement
        sidebar_entries.push(widget::button(widget::text("New Workspace")).into());
    }
    let sidebar_entries_container: cosmic::Element<'_, _> = match layout {
        WorkspaceLayout::Vertical => column(sidebar_entries).into(),
        WorkspaceLayout::Horizontal => row(sidebar_entries).into(),
    };
    widget::container(sidebar_entries_container)
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into()
}

pub(crate) fn toplevel_preview(toplevel: &Toplevel) -> cosmic::Element<Msg> {
    column![
        close_button(Msg::CloseToplevel(toplevel.handle.clone())),
        widget::button(capture_image(toplevel.img.as_ref()))
            .selected(
                toplevel
                    .info
                    .state
                    .contains(&zcosmic_toplevel_handle_v1::State::Activated)
            )
            .style(cosmic::theme::Button::Image)
            .on_press(Msg::ActivateToplevel(toplevel.handle.clone())),
        widget::text(&toplevel.info.title)
            .horizontal_alignment(iced::alignment::Horizontal::Center)
    ]
    .width(iced::Length::Fill)
    .into()
}

fn toplevel_previews_entry<'a>(
    toplevel: &'a Toplevel,
    output: &'a wl_output::WlOutput,
) -> cosmic::Element<'a, Msg> {
    iced::widget::dnd_source(toplevel_preview(toplevel))
        .on_drag(|size| {
            Msg::StartDrag(
                size,
                DragSurface::Toplevel {
                    handle: toplevel.handle.clone(),
                    output: output.clone(),
                },
            )
        })
        .on_finished(Msg::SourceFinished)
        .on_cancelled(Msg::SourceFinished)
        .into()
}

fn toplevel_previews<'a>(
    toplevels: impl Iterator<Item = &'a Toplevel>,
    output: &'a wl_output::WlOutput,
) -> cosmic::Element<'a, Msg> {
    row(toplevels
        .map(|t| toplevel_previews_entry(t, output))
        .collect())
    .width(iced::Length::FillPortion(4))
    .height(iced::Length::Fill)
    .spacing(16)
    .align_items(iced::Alignment::Center)
    .into()
}

fn capture_image(image: Option<&CaptureImage>) -> cosmic::Element<'_, Msg> {
    if let Some(image) = image {
        widget::Image::new(image.img.clone()).into()
    } else {
        widget::Image::new(widget::image::Handle::from_pixels(1, 1, vec![0, 0, 0, 255])).into()
    }
}
