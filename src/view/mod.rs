use cosmic::{
    cctk::{
        cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
        wayland_client::protocol::wl_output,
    },
    iced::{
        self,
        advanced::layout::flex::Axis,
        clipboard::mime::AllowedMimeTypes,
        widget::{column, row},
        Border, Length,
    },
    iced_core::{text::Wrapping, Shadow},
    iced_winit::platform_specific::wayland::subsurface_widget::Subsurface,
    widget, Apply,
};
use cosmic_comp_config::workspace::WorkspaceLayout;
use std::collections::HashMap;

use crate::{
    backend::{self, CaptureImage},
    dnd::{DragSurface, DragToplevel, DragWorkspace, DropTarget},
    App, LayerSurface, Msg, Toplevel, Workspace,
};

fn dnd_destination_for_target<T>(
    target: DropTarget,
    child: cosmic::Element<'_, Msg>,
    on_finish: impl Fn(T) -> Msg + 'static,
) -> cosmic::Element<'_, Msg>
where
    T: AllowedMimeTypes,
{
    let target2 = target.clone();
    cosmic::widget::dnd_destination::dnd_destination_for_data(
        child,
        move |data: Option<T>, _action| match data {
            Some(data) => on_finish(data),
            None => Msg::Ignore,
        },
    )
    .drag_id(target.drag_id())
    .on_enter(move |actions, mime, pos| Msg::DndEnter(target.clone(), actions, mime, pos))
    .on_leave(move || Msg::DndLeave(target2.clone()))
    .into()
}

pub(crate) fn layer_surface<'a>(
    app: &'a App,
    surface: &'a LayerSurface,
) -> cosmic::Element<'a, Msg> {
    let mut drop_target = None;
    if let Some(DropTarget::WorkspaceSidebarEntry(workspace, output)) = &app.drop_target {
        if output == &surface.output {
            drop_target = Some(workspace);
        }
    }
    let mut drag_toplevel = None;
    if let Some((DragSurface::Toplevel(handle), _)) = &app.drag_surface {
        drag_toplevel = Some(handle);
    }
    let layout = app.conf.workspace_config.workspace_layout;
    let sidebar = workspaces_sidebar(
        app.workspaces_for_output(&surface.output),
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
                    .is_some_and(|x| x.is_active())
            })
        }),
        layout,
        drag_toplevel,
    );
    // TODO multiple active workspaces? Not currently supported by cosmic.
    let first_active_workspace = app
        .workspaces_for_output(&surface.output)
        .find(|w| w.is_active());
    let toplevels = if let Some(workspace) = first_active_workspace {
        dnd_destination_for_target(
            DropTarget::OutputToplevels(workspace.handle().clone(), surface.output.clone()),
            toplevels,
            Msg::DndToplevelDrop,
        )
    } else {
        // Shouldn't happen, but no drag destination if no active workspace found for output
        cosmic::Element::from(toplevels)
    };
    let container = match layout {
        WorkspaceLayout::Vertical => widget::layer_container(
            row![sidebar, toplevels]
                .spacing(12)
                .height(Length::Fill)
                .width(Length::Fill),
        ),
        WorkspaceLayout::Horizontal => widget::layer_container(
            column![sidebar, toplevels]
                .spacing(12)
                .height(Length::Fill)
                .width(Length::Fill),
        ),
    };
    let output = surface.output.clone();
    widget::mouse_area(container)
        .on_scroll(move |delta| Msg::OnScroll(output.clone(), delta))
        .into()
}

fn close_button(on_press: Msg) -> cosmic::Element<'static, Msg> {
    widget::button::custom(widget::icon::from_name("window-close-symbolic").size(16))
        .class(cosmic::theme::Button::Destructive)
        .on_press(on_press)
        .into()
}

fn workspace_item_appearance(
    theme: &cosmic::Theme,
    is_active: bool,
    hovered: bool,
) -> cosmic::widget::button::Style {
    let cosmic = theme.cosmic();
    let mut appearance = cosmic::widget::button::Style::new();
    appearance.border_radius = cosmic
        .corner_radii
        .radius_s
        .map(|x| if x < 4.0 { x } else { x + 4.0 })
        .into();
    if is_active {
        appearance.border_width = 4.0;
        appearance.border_color = cosmic.accent.base.into();
    }
    if hovered {
        appearance.background = Some(iced::Background::Color(cosmic.button.base.into()));
    }
    appearance
}

fn workspace_item(
    workspace: &Workspace,
    _output: &wl_output::WlOutput,
    layout: WorkspaceLayout,
    is_drop_target: bool,
) -> cosmic::Element<'static, Msg> {
    let (image, image_height) = if let Some(img) = workspace.img.as_ref() {
        let is_rotated = matches!(
            img.transform,
            wl_output::Transform::_90
                | wl_output::Transform::_270
                | wl_output::Transform::Flipped90
                | wl_output::Transform::Flipped270
        );
        let (effective_width, effective_height) = if is_rotated {
            // If rotated, swap width and height
            (img.height, img.width)
        } else {
            (img.width, img.height)
        };

        let fixed_size = 126.0;
        if effective_width > effective_height {
            (
                // Landscape: fix height
                widget::container(capture_image(Some(img), 1.0)).max_height(fixed_size),
                fixed_size,
            )
        } else {
            (
                // Portrait: fix width
                widget::container(capture_image(Some(img), 1.0)).max_width(fixed_size),
                fixed_size * effective_height as f32 / effective_width as f32,
            )
        }
    } else {
        (
            widget::container(capture_image(None, 1.0))
                .max_width(224.0)
                .max_height(126.0),
            126.0,
        )
    };

    let workspace_name = widget::text::body(fl!(
        "workspace",
        HashMap::from([("number", &workspace.info.name)])
    ));

    // Needed to prevent text getting pushed out when scaling on Vertical layout
    let content = match layout {
        WorkspaceLayout::Horizontal => column![image, workspace_name]
            .align_x(iced::Alignment::Center)
            .spacing(4)
            .apply(widget::container),
        WorkspaceLayout::Vertical => column![image.height(Length::Fill), workspace_name]
            .align_x(iced::Alignment::Center)
            .spacing(4)
            .apply(widget::container)
            .max_height(image_height + 21.0 + 4.0), // text height + spacing
    };

    let is_active = workspace.is_active();
    // TODO editable name?
    widget::button::custom(content)
        .selected(workspace.is_active())
        .class(cosmic::theme::Button::Custom {
            active: Box::new(move |_focused, theme| {
                workspace_item_appearance(theme, is_active, is_drop_target)
            }),
            disabled: Box::new(|_theme| unreachable!()),
            hovered: Box::new(move |_focused, theme| {
                workspace_item_appearance(theme, is_active, true)
            }),
            pressed: Box::new(move |_focused, theme| {
                workspace_item_appearance(theme, is_active, true)
            }),
        })
        .on_press(Msg::ActivateWorkspace(workspace.handle().clone()))
        .padding(8)
        .into()
}

fn workspace_sidebar_entry<'a>(
    workspace: &'a Workspace,
    output: &'a wl_output::WlOutput,
    layout: WorkspaceLayout,
    is_drop_target: bool,
) -> cosmic::Element<'a, Msg> {
    /* XXX
    let mouse_interaction = if is_drop_target {
        iced::mouse::Interaction::Crosshair
    } else {
        iced::mouse::Interaction::Idle
    };
    */
    let item = workspace_item(workspace, output, layout, is_drop_target);
    /* TODO allow moving workspaces (needs compositor support)
    let workspace_clone = workspace.clone(); // TODO avoid clone
    let output_clone = output.clone();
    let source = cosmic::widget::dnd_source(item)
        .drag_threshold(5.)
        .drag_content(|| DragWorkspace {})
        .drag_icon(move |offset| {
            (
                workspace_item(&workspace_clone, &output_clone, false).map(|_| ()),
                cosmic::iced_core::widget::tree::State::None,
                -offset,
            )
        })
        .on_start(Some(Msg::StartDrag(DragSurface::Workspace(
            workspace.handle.clone(),
        ))))
        .on_finish(Some(Msg::SourceFinished))
        .on_cancel(Some(Msg::SourceFinished))
        .into();
    */
    //crate::widgets::mouse_interaction_wrapper(
    //   mouse_interaction,
    dnd_destination_for_target(
        DropTarget::WorkspaceSidebarEntry(workspace.handle().clone(), output.clone()),
        item,
        Msg::DndToplevelDrop,
    )
}

fn workspaces_sidebar<'a>(
    workspaces: impl Iterator<Item = &'a Workspace>,
    output: &'a wl_output::WlOutput,
    layout: WorkspaceLayout,
    drop_target: Option<&backend::ExtWorkspaceHandleV1>,
) -> cosmic::Element<'a, Msg> {
    let sidebar_entries = workspaces
        .map(|w| workspace_sidebar_entry(w, output, layout, drop_target == Some(w.handle())))
        .collect();
    let (axis, width, height) = match layout {
        WorkspaceLayout::Vertical => (Axis::Vertical, Length::Shrink, Length::Fill),
        WorkspaceLayout::Horizontal => (Axis::Horizontal, Length::Fill, Length::Shrink),
    };
    let sidebar_entries_container =
        widget::container(crate::widgets::workspace_bar(sidebar_entries, axis)).padding(8.0);

    widget::container(
        widget::container(sidebar_entries_container)
            .width(width)
            .height(height)
            .class(cosmic::theme::Container::custom(|theme| {
                cosmic::iced::widget::container::Style {
                    text_color: Some(theme.cosmic().on_bg_color().into()),
                    icon_color: Some(theme.cosmic().on_bg_color().into()),
                    background: Some(iced::Color::from(theme.cosmic().background.base).into()),
                    border: Border {
                        radius: theme
                            .cosmic()
                            .radius_s()
                            .map(|x| if x < 4.0 { x } else { x + 8.0 })
                            .into(),
                        ..Default::default()
                    },
                    shadow: Shadow::default(),
                }
            })),
    )
    .padding(8)
    .into()
}

fn toplevel_preview(toplevel: &Toplevel, is_being_dragged: bool) -> cosmic::Element<'static, Msg> {
    let cosmic::cosmic_theme::Spacing {
        space_xxs, space_s, ..
    } = cosmic::theme::active().cosmic().spacing;

    let label = widget::text::body(toplevel.info.title.clone()).wrapping(Wrapping::None);
    let label = if let Some(icon) = &toplevel.icon {
        row![
            widget::icon(widget::icon::from_path(icon.clone())).size(24),
            label
        ]
        .spacing(4)
    } else {
        row![label]
    }
    .align_y(iced::Alignment::Center);
    let alpha = if is_being_dragged { 0.5 } else { 1.0 };
    crate::widgets::size_cross_nth(
        vec![
            row![
                widget::button::custom(label)
                    .on_press(Msg::ActivateToplevel(toplevel.handle.clone()))
                    .class(cosmic::theme::Button::Icon)
                    .padding([space_xxs, space_s])
                    .apply(widget::container)
                    .class(cosmic::theme::Container::custom(|theme| {
                        cosmic::iced::widget::container::Style {
                            background: Some(
                                iced::Color::from(theme.cosmic().background.component.base).into(),
                            ),
                            border: Border {
                                color: theme.cosmic().bg_divider().into(),
                                width: 1.0,
                                radius: theme.cosmic().radius_xl().into(),
                            },
                            ..Default::default()
                        }
                    }))
                    .apply(widget::container)
                    .width(Length::FillPortion(5)),
                widget::horizontal_space().width(Length::Fixed(8.0)),
                close_button(Msg::CloseToplevel(toplevel.handle.clone()))
            ]
            .padding([0, 0, 4, 0])
            .align_y(iced::Alignment::Center)
            .into(),
            widget::button::custom(capture_image(toplevel.img.as_ref(), alpha))
                .selected(
                    toplevel
                        .info
                        .state
                        .contains(&zcosmic_toplevel_handle_v1::State::Activated),
                )
                .class(cosmic::theme::Button::Image)
                .on_press(Msg::ActivateToplevel(toplevel.handle.clone()))
                .into(),
        ],
        Axis::Vertical,
        1, // Allocate width to match capture image
    )
    //.spacing(4)
    //.align_items(iced::Alignment::Center)
    //.width(Length::Fill)
    .into()
}

fn toplevel_previews_entry(
    toplevel: &Toplevel,
    is_being_dragged: bool,
) -> cosmic::Element<'_, Msg> {
    // Dragged window still takes up space until moved, but isn't rendered while drag surface is
    // shown.
    let preview = crate::widgets::visibility_wrapper(
        toplevel_preview(toplevel, is_being_dragged),
        !is_being_dragged,
    );
    let toplevel2 = toplevel.clone();
    cosmic::widget::dnd_source::<_, DragToplevel>(preview)
        .drag_threshold(5.)
        .drag_content(|| DragToplevel {})
        // XXX State?
        .drag_icon(move |offset| {
            (
                toplevel_preview(&toplevel2, true).map(|_| ()),
                cosmic::iced_core::widget::tree::State::None,
                -offset,
            )
        })
        .on_start(Some(Msg::StartDrag(
            //size,
            DragSurface::Toplevel(toplevel.handle.clone()),
        )))
        .on_finish(Some(Msg::SourceFinished))
        .on_cancel(Some(Msg::SourceFinished))
        .into()
}

fn toplevel_previews<'a>(
    toplevels: impl Iterator<Item = &'a Toplevel>,
    layout: WorkspaceLayout,
    drag_toplevel: Option<&'a backend::ExtForeignToplevelHandleV1>,
) -> cosmic::Element<'a, Msg> {
    let (width, height) = match layout {
        WorkspaceLayout::Vertical => (Length::FillPortion(4), Length::Fill),
        WorkspaceLayout::Horizontal => (Length::Fill, Length::FillPortion(4)),
    };
    let entries = toplevels
        .map(|t| toplevel_previews_entry(t, drag_toplevel == Some(&t.handle)))
        .collect();
    //row(entries)
    widget::mouse_area(
        widget::container(crate::widgets::toplevels(entries))
            .align_x(iced::alignment::Horizontal::Center)
            .width(width)
            .height(height)
            //.spacing(16)
            .padding(12),
    )
    .on_press(Msg::Close)
    //.align_items(iced::Alignment::Center)
    .into()
}

fn capture_image(image: Option<&CaptureImage>, alpha: f32) -> cosmic::Element<'static, Msg> {
    if let Some(image) = image {
        #[cfg(feature = "no-subsurfaces")]
        {
            // TODO alpha, transform
            widget::Image::new(image.image.clone()).into()
        }
        #[cfg(not(feature = "no-subsurfaces"))]
        {
            Subsurface::new(image.wl_buffer.clone())
                .alpha(alpha)
                .transform(image.transform)
                .into()
        }
    } else {
        widget::Image::new(widget::image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255])).into()
    }
}
