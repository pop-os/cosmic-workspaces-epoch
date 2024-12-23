use std::collections::HashMap;

use cctk::{
    cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
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
    iced_core::{text::Wrapping, Shadow},
    iced_winit::platform_specific::wayland::subsurface_widget::Subsurface,
    widget, Apply,
};
use cosmic_bg_config::Source;
use cosmic_comp_config::workspace::WorkspaceLayout;

use crate::{
    backend::{self, CaptureImage},
    App, DragSurface, DragToplevel, LayerSurface, Msg, Toplevel, Workspace,
};

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
    let mut drag_toplevel = None;
    if let Some((DragSurface::Toplevel { handle, .. }, _)) = &app.drag_surface {
        drag_toplevel = Some(handle);
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
        drag_toplevel,
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
    container.into()
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

fn workspace_item<'a>(
    workspace: &'a Workspace,
    output: &wl_output::WlOutput,
    is_drop_target: bool,
) -> cosmic::Element<'a, Msg> {
    let image = capture_image(workspace.img_for_output.get(output), 1.0);
    let is_active = workspace.is_active;
    // TODO editable name?
    widget::button::custom(
        column![
            image,
            widget::text::body(fl!(
                "workspace",
                HashMap::from([("number", &workspace.name)])
            ))
        ]
        .align_x(iced::Alignment::Center)
        .spacing(4),
    )
    .selected(workspace.is_active)
    .class(cosmic::theme::Button::Custom {
        active: Box::new(move |_focused, theme| {
            workspace_item_appearance(theme, is_active, is_drop_target)
        }),
        disabled: Box::new(|_theme| unreachable!()),
        hovered: Box::new(move |_focused, theme| workspace_item_appearance(theme, is_active, true)),
        pressed: Box::new(move |_focused, theme| workspace_item_appearance(theme, is_active, true)),
    })
    .on_press(Msg::ActivateWorkspace(workspace.handle.clone()))
    .padding(8)
    .width(iced::Length::Fixed(240.0))
    .into()
}

fn workspace_sidebar_entry<'a>(
    workspace: &'a Workspace,
    output: &'a wl_output::WlOutput,
    is_drop_target: bool,
    drag_id: u64,
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
    let workspace_handle = workspace.handle.clone();
    let workspace_handle2 = workspace_handle.clone();
    let output_clone = output.clone();
    let output_clone2 = output.clone();
    cosmic::widget::dnd_destination::dnd_destination_for_data(
        workspace_item(workspace, output, is_drop_target),
        |data: Option<DragToplevel>, _action| match data {
            Some(toplevel) => Msg::DndWorkspaceDrop(toplevel),
            None => Msg::Ignore,
        },
    )
    .drag_id(drag_id)
    .on_enter(move |actions, mime, pos| {
        Msg::DndWorkspaceEnter(
            workspace_handle.clone(),
            output_clone.clone(),
            actions,
            mime,
            pos,
        )
    })
    .on_leave(move || Msg::DndWorkspaceLeave(workspace_handle2.clone(), output_clone2.clone()))
    //)
    .into()
}

fn workspaces_sidebar<'a>(
    workspaces: impl Iterator<Item = &'a Workspace>,
    output: &'a wl_output::WlOutput,
    layout: WorkspaceLayout,
    drop_target: Option<&backend::ZcosmicWorkspaceHandleV1>,
) -> cosmic::Element<'a, Msg> {
    let sidebar_entries = workspaces
        .enumerate()
        .map(|(i, w)| workspace_sidebar_entry(w, output, drop_target == Some(&w.handle), i as u64))
        .collect();
    let axis = match layout {
        WorkspaceLayout::Vertical => Axis::Vertical,
        WorkspaceLayout::Horizontal => Axis::Horizontal,
    };
    let sidebar_entries_container =
        widget::container(crate::widgets::workspace_bar(sidebar_entries, axis)).padding(8.0);

    let (width, height) = match layout {
        WorkspaceLayout::Vertical => (iced::Length::Fixed(256.0), iced::Length::Shrink),
        WorkspaceLayout::Horizontal => (iced::Length::Shrink, iced::Length::Fill),
    };
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
    crate::widgets::toplevel_item(
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
                    .width(iced::Length::FillPortion(5)),
                widget::horizontal_space().width(iced::Length::Fixed(8.0)),
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
    )
    //.spacing(4)
    //.align_items(iced::Alignment::Center)
    //.width(iced::Length::Fill)
    .into()
}

fn toplevel_previews_entry<'a>(
    toplevel: &'a Toplevel,
    output: &'a wl_output::WlOutput,
    is_being_dragged: bool,
) -> cosmic::Element<'a, Msg> {
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
            //offset,
            DragSurface::Toplevel {
                handle: toplevel.handle.clone(),
                output: output.clone(),
            },
        )))
        .on_finish(Some(Msg::SourceFinished))
        .on_cancel(Some(Msg::SourceFinished))
        .into()
}

fn toplevel_previews<'a>(
    toplevels: impl Iterator<Item = &'a Toplevel>,
    output: &'a wl_output::WlOutput,
    layout: WorkspaceLayout,
    drag_toplevel: Option<&'a backend::ZcosmicToplevelHandleV1>,
) -> cosmic::Element<'a, Msg> {
    let (width, height) = match layout {
        WorkspaceLayout::Vertical => (iced::Length::FillPortion(4), iced::Length::Fill),
        WorkspaceLayout::Horizontal => (iced::Length::Fill, iced::Length::FillPortion(4)),
    };
    let entries = toplevels
        .map(|t| toplevel_previews_entry(t, output, drag_toplevel == Some(&t.handle)))
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

fn bg_element<'a>(
    bg_state: &'a cosmic_bg_config::state::State,
    output_name: &'a str,
) -> cosmic::Element<'a, Msg> {
    let bg_source = bg_state
        .wallpapers
        .iter()
        .find(|(n, _)| n == output_name)
        .map(|(_, v)| v.clone());
    match bg_source {
        Some(Source::Path(path)) => widget::image::Image::<widget::image::Handle>::new(
            widget::image::Handle::from_path(path),
        )
        .content_fit(iced::ContentFit::Cover)
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into(),
        Some(Source::Color(color)) => widget::layer_container(widget::horizontal_space())
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .class(cosmic::theme::Container::Custom(Box::new(move |_| {
                let color = color.clone();
                cosmic::iced::widget::container::Style {
                    background: Some(match color {
                        cosmic_bg_config::Color::Single(c) => {
                            iced::Background::Color(cosmic::iced::Color::new(c[0], c[1], c[2], 1.0))
                        }
                        cosmic_bg_config::Color::Gradient(cosmic_bg_config::Gradient {
                            colors,
                            radius,
                        }) => {
                            let stop_increment = 1.0 / (colors.len() - 1) as f32;
                            let mut stop = 0.0;

                            let mut linear = iced::gradient::Linear::new(iced::Degrees(radius));

                            for &[r, g, b] in colors.iter() {
                                linear =
                                    linear.add_stop(stop, cosmic::iced::Color::from_rgb(r, g, b));
                                stop += stop_increment;
                            }

                            iced::Background::Gradient(cosmic::iced_core::Gradient::Linear(linear))
                        }
                    }),
                    ..Default::default()
                }
            })))
            .into(),
        None => {
            widget::image::Image::<widget::image::Handle>::new(widget::image::Handle::from_path(
                "/usr/share/backgrounds/pop/kate-hazen-COSMIC-desktop-wallpaper.png",
            ))
            .content_fit(iced::ContentFit::Cover)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .into()
        }
    }
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
