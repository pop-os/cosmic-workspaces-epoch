use cctk::{
    cosmic_protocols::{
        toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
        workspace::v1::client::zcosmic_workspace_handle_v1,
    },
    wayland_client::protocol::wl_output,
};
use cosmic::{
    cctk,
    iced::{
        self,
        advanced::layout::flex::Axis,
        widget::{column, row},
        Border,
    },
    iced_core::Shadow,
    iced_sctk::subsurface_widget::Subsurface,
    widget,
};
use cosmic_comp_config::workspace::WorkspaceLayout;

use crate::{wayland::CaptureImage, App, DragSurface, LayerSurface, Msg, Toplevel, Workspace};

pub(crate) fn layer_surface<'a>(
    app: &'a App,
    surface: &'a LayerSurface,
) -> cosmic::Element<'a, Msg> {
    let mut drop_target = None;
    if let Some((workspace, output)) = &app.drop_target {
        if output == &surface.output {
            drop_target = Some(workspace);
        }
    }
    let layout = app.conf.workspace_config.workspace_layout;
    let sidebar = workspaces_sidebar(
        app.workspaces
            .iter()
            .filter(|i| i.outputs.contains(&surface.output)),
        &surface.output,
        layout,
        drop_target,
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
        layout,
    );
    let container = match layout {
        WorkspaceLayout::Vertical => widget::layer_container(
            row![sidebar, toplevels]
                .spacing(12)
                .height(iced::Length::Fill)
                .width(iced::Length::Fill),
        ),
        WorkspaceLayout::Horizontal => widget::layer_container(
            column![sidebar, toplevels]
                .spacing(12)
                .height(iced::Length::Fill)
                .width(iced::Length::Fill),
        ),
    };
    crate::widgets::image_bg(container).into()
}

fn close_button(on_press: Msg) -> cosmic::Element<'static, Msg> {
    widget::container(
        widget::button(widget::icon::from_name("window-close-symbolic").size(16))
            .style(cosmic::theme::Button::Destructive)
            .on_press(on_press),
    )
    .align_x(iced::alignment::Horizontal::Right)
    .width(iced::Length::Fill)
    .into()
}

pub(crate) fn workspace_item<'a>(
    workspace: &'a Workspace,
    output: &wl_output::WlOutput,
) -> cosmic::Element<'a, Msg> {
    let image = capture_image(workspace.img_for_output.get(output));
    column![
        // TODO editable name?
        widget::button(column![image, widget::text(&workspace.name)])
            .selected(workspace.is_active)
            .style(cosmic::theme::Button::Image)
            .on_press(Msg::ActivateWorkspace(workspace.handle.clone())),
    ]
    .spacing(4)
    //.height(iced::Length::Fill)
    .into()
}

fn workspace_sidebar_entry<'a>(
    workspace: &'a Workspace,
    output: &'a wl_output::WlOutput,
    _is_drop_target: bool,
) -> cosmic::Element<'a, Msg> {
    /* XXX
    let mouse_interaction = if is_drop_target {
        iced::mouse::Interaction::Crosshair
    } else {
        iced::mouse::Interaction::Idle
    };
    */
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
    //crate::widgets::mouse_interaction_wrapper(
    //   mouse_interaction,
    iced::widget::dnd_listener(workspace_item(workspace, output))
        .on_enter(|actions, mime, pos| {
            Msg::DndWorkspaceEnter(workspace.handle.clone(), output.clone(), actions, mime, pos)
        })
        .on_exit(Msg::DndWorkspaceLeave)
        .on_drop(Msg::DndWorkspaceDrop)
        .on_data(Msg::DndWorkspaceData)
        //)
        .into()
}

fn workspaces_sidebar<'a>(
    workspaces: impl Iterator<Item = &'a Workspace>,
    output: &'a wl_output::WlOutput,
    layout: WorkspaceLayout,
    drop_target: Option<&zcosmic_workspace_handle_v1::ZcosmicWorkspaceHandleV1>,
) -> cosmic::Element<'a, Msg> {
    let sidebar_entries = workspaces
        .map(|w| workspace_sidebar_entry(w, output, drop_target == Some(&w.handle)))
        .collect();
    let axis = match layout {
        WorkspaceLayout::Vertical => Axis::Vertical,
        WorkspaceLayout::Horizontal => Axis::Horizontal,
    };
    let sidebar_entries_container =
        widget::container(crate::widgets::workspace_bar(sidebar_entries, axis)).padding(12.0);
    /*
    let new_workspace_button = widget::button(
        widget::container(row![
            widget::icon::from_name("list-add-symbolic").symbolic(true),
            widget::text(fl!("new-workspace"))
        ])
        .width(iced::Length::Fill)
        .align_x(iced::alignment::Horizontal::Center),
    )
    .on_press(Msg::NewWorkspace)
    .width(iced::Length::Fill);
    let bar: cosmic::Element<_> = if amount != WorkspaceAmount::Dynamic {
        match layout {
            WorkspaceLayout::Vertical => {
                column![sidebar_entries_container, new_workspace_button,].into()
            }
            WorkspaceLayout::Horizontal => {
                row![sidebar_entries_container, new_workspace_button,].into()
            }
        }
    } else {
        sidebar_entries_container.into()
    };
    */
    // Shrink?
    let (width, height) = match layout {
        WorkspaceLayout::Vertical => (iced::Length::Fill, iced::Length::Shrink),
        WorkspaceLayout::Horizontal => (iced::Length::Shrink, iced::Length::Fill),
    };
    widget::container(
        widget::container(sidebar_entries_container)
            .width(width)
            .height(height)
            .style(cosmic::theme::Container::custom(|theme| {
                cosmic::iced_style::container::Appearance {
                    text_color: Some(theme.cosmic().on_bg_color().into()),
                    icon_color: Some(theme.cosmic().on_bg_color().into()),
                    background: Some(iced::Color::from(theme.cosmic().background.base).into()),
                    border: Border {
                        radius: (12.0).into(),
                        width: 0.0,
                        color: iced::Color::TRANSPARENT,
                    },
                    shadow: Shadow::default(),
                }
            })),
    )
    .width(width)
    .height(height)
    .padding(24.0)
    .into()
}

pub(crate) fn toplevel_preview(toplevel: &Toplevel) -> cosmic::Element<Msg> {
    let label = widget::text(&toplevel.info.title);
    let label = if let Some(icon) = &toplevel.icon {
        row![widget::icon(widget::icon::from_path(icon.clone())), label].spacing(4)
    } else {
        row![label]
    }
    .padding(4);
    crate::widgets::workspace_item(
        vec![
            close_button(Msg::CloseToplevel(toplevel.handle.clone())).into(),
            widget::button(capture_image(toplevel.img.as_ref()))
                .selected(
                    toplevel
                        .info
                        .state
                        .contains(&zcosmic_toplevel_handle_v1::State::Activated),
                )
                .style(cosmic::theme::Button::Image)
                .on_press(Msg::ActivateToplevel(toplevel.handle.clone()))
                .into(),
            widget::button(label)
                .on_press(Msg::ActivateToplevel(toplevel.handle.clone()))
                .into(),
        ],
        Axis::Vertical,
    )
    //.spacing(4)
    //.align_items(iced::Alignment::Center)
    //.width(iced::Length::Fill)
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
    layout: WorkspaceLayout,
) -> cosmic::Element<'a, Msg> {
    let (width, height) = match layout {
        WorkspaceLayout::Vertical => (iced::Length::FillPortion(4), iced::Length::Fill),
        WorkspaceLayout::Horizontal => (iced::Length::Fill, iced::Length::FillPortion(4)),
    };
    let entries = toplevels
        .map(|t| toplevel_previews_entry(t, output))
        .collect();
    //row(entries)
    widget::container(crate::widgets::toplevels(entries))
        .align_x(iced::alignment::Horizontal::Center)
        .width(width)
        .height(height)
        //.spacing(16)
        .padding(12)
        //.align_items(iced::Alignment::Center)
        .into()
}

fn capture_image(image: Option<&CaptureImage>) -> cosmic::Element<'_, Msg> {
    if let Some(image) = image {
        Subsurface::new(image.width, image.height, &image.wl_buffer).into()
    } else {
        widget::Image::new(widget::image::Handle::from_pixels(1, 1, vec![0, 0, 0, 255])).into()
    }
}
