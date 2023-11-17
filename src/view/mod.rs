use cctk::wayland_client::protocol::wl_output;
use cosmic::{
    iced::{
        self,
        widget::{column, row},
    },
    widget,
};
use cosmic_comp_config::workspace::WorkspaceLayout;

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
    );
    let toplevels = toplevel_previews(app.toplevels.iter().filter(|i| {
        if !i.info.output.contains(&surface.output) {
            return false;
        }

        i.info.workspace.iter().any(|workspace| {
            app.workspace_for_handle(workspace)
                .map_or(false, |x| x.is_active)
        })
    }));
    match layout {
        WorkspaceLayout::Vertical => row![sidebar, toplevels]
            .spacing(12)
            .height(iced::Length::Fill)
            .width(iced::Length::Fill)
            .into(),
        WorkspaceLayout::Horizontal => column![sidebar, toplevels]
            .spacing(12)
            .height(iced::Length::Fill)
            .width(iced::Length::Fill)
            .into(),
    }
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
    // TODO style
    let theme = if workspace.is_active {
        cosmic::theme::Button::Suggested
    } else {
        cosmic::theme::Button::Standard
    };
    column![
        close_button(Msg::CloseWorkspace(workspace.handle.clone())),
        widget::button(column![
            capture_image(workspace.img_for_output.get(output)),
            widget::text(&workspace.name)
        ])
        .style(theme)
        .on_press(Msg::ActivateWorkspace(workspace.handle.clone())),
    ]
    .height(iced::Length::Fill)
    .into()
}

fn workspace_sidebar_entry<'a>(
    workspace: &'a Workspace,
    output: &'a wl_output::WlOutput,
) -> cosmic::Element<'a, Msg> {
    iced::widget::dnd_source(workspace_item(workspace, output))
        .on_drag(|size| {
            Msg::StartDrag(
                size,
                DragSurface::Workspace {
                    name: workspace.name.to_string(),
                    output: output.clone(),
                },
            )
        })
        .on_finished(Msg::SourceFinished)
        .on_cancelled(Msg::SourceFinished)
        .into()
}

fn workspaces_sidebar<'a>(
    workspaces: impl Iterator<Item = &'a Workspace>,
    output: &'a wl_output::WlOutput,
    layout: WorkspaceLayout,
) -> cosmic::Element<'a, Msg> {
    let sidebar_entries = workspaces
        .map(|w| workspace_sidebar_entry(w, output))
        .collect();
    let sidebar_entries_container: cosmic::Element<'_, _> = match layout {
        WorkspaceLayout::Vertical => column(sidebar_entries).into(),
        WorkspaceLayout::Horizontal => row(sidebar_entries).into(),
    };
    widget::container(
        iced::widget::dnd_listener(sidebar_entries_container)
            .on_enter(Msg::DndWorkspaceEnter)
            .on_exit(Msg::DndWorkspaceLeave)
            .on_drop(Msg::DndWorkspaceDrop)
            .on_data(Msg::DndWorkspaceData),
    )
    .width(iced::Length::Fill)
    .height(iced::Length::Fill)
    .into()

    // New workspace
}

fn toplevel_preview(toplevel: &Toplevel) -> cosmic::Element<Msg> {
    column![
        close_button(Msg::CloseToplevel(toplevel.handle.clone())),
        widget::button(capture_image(toplevel.img.as_ref()))
            .on_press(Msg::ActivateToplevel(toplevel.handle.clone())),
        widget::text(&toplevel.info.title)
            .horizontal_alignment(iced::alignment::Horizontal::Center)
    ]
    .width(iced::Length::Fill)
    .into()
}

fn toplevel_previews<'a>(
    toplevels: impl Iterator<Item = &'a Toplevel>,
) -> cosmic::Element<'a, Msg> {
    row(toplevels.map(toplevel_preview).collect())
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
