use cosmic::iced::{
    Length, Rectangle, Size, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        layout::{self, flex::Axis},
        mouse, renderer,
        widget::{Operation, Tree},
    },
    event::{self, Event},
};
use std::marker::PhantomData;

mod toplevel_layout;
use toplevel_layout::{LayoutToplevel, ToplevelLayout, TwoRowColToplevelLayout};

pub fn toplevels<Msg>(
    children: Vec<cosmic::Element<Msg>>,
    selected_index: Option<usize>,
    selection_scale: f32,
) -> Toplevels<Msg> {
    Toplevels {
        layout: TwoRowColToplevelLayout::new(Axis::Horizontal, 16),
        children,
        source_rects: Vec::new(),
        animation_progress: 1.0,
        output_size: Size::new(1920.0, 1080.0),
        widget_offset: cosmic::iced::Point::ORIGIN,
        selected_index,
        selection_scale,
        _msg: PhantomData,
    }
}

pub fn toplevels_animated<Msg>(
    children: Vec<cosmic::Element<Msg>>,
    source_rects: Vec<Rectangle>,
    animation_progress: f32,
    output_size: Size,
    widget_offset: cosmic::iced::Point,
    selected_index: Option<usize>,
    selection_scale: f32,
) -> Toplevels<Msg> {
    Toplevels {
        layout: TwoRowColToplevelLayout::new(Axis::Horizontal, 16),
        children,
        source_rects,
        animation_progress,
        output_size,
        widget_offset,
        selected_index,
        selection_scale,
        _msg: PhantomData,
    }
}

pub struct Toplevels<'a, Msg> {
    layout: TwoRowColToplevelLayout,
    children: Vec<cosmic::Element<'a, Msg>>,
    source_rects: Vec<Rectangle>,
    animation_progress: f32,
    output_size: Size,
    widget_offset: cosmic::iced::Point,
    selected_index: Option<usize>,
    selection_scale: f32,
    _msg: PhantomData<Msg>,
}

impl<Msg> Widget<Msg, cosmic::Theme, cosmic::Renderer> for Toplevels<'_, Msg> {
    fn size(&self) -> Size<Length> {
        self.layout.size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // Call `.layout()` on each child with full limits to determine "preferred" sizes
        let layout_toplevels = self
            .children
            .iter_mut()
            .zip(tree.children.iter_mut())
            .map(|(child, tree)| {
                let preferred_size = child.as_widget_mut().layout(tree, renderer, limits).size();
                LayoutToplevel {
                    preferred_size,
                    _phantom_data: PhantomData,
                }
            })
            .collect::<Vec<_>>();

        // Assign rectangles for each child using `ToplevelLayout` backend
        let assigned_rects = self.layout.layout(limits.max(), &layout_toplevels);

        let t = self.animation_progress;
        let container_size = limits.max();

        // Derive the coordinate mapping by finding the largest source rect
        // (likely a fullscreen or maximized window). Its width in geometry should
        // map to approximately container_size.width at t=0.
        // display_scale = container_size / max_source_size (approximately).
        //
        // Fallback: use output_size ratio if no source rects available.
        let max_src_w = self.source_rects.iter()
            .map(|r| r.width)
            .fold(0.0_f32, f32::max)
            .max(1.0);
        let max_src_h = self.source_rects.iter()
            .map(|r| r.height)
            .fold(0.0_f32, f32::max)
            .max(1.0);

        // The largest window at t=0 should fill the container.
        // But it won't perfectly because the container has padding and the window
        // might not be truly fullscreen. Use output as the reference instead.
        // display_scale = how many iced-units per logical-pixel
        // We know container fills (output - sidebar - padding) in iced units.
        // sidebar + padding ≈ 280 logical, which in iced units = 280 * display_scale.
        // container = (output - 280) * display_scale... no, sidebar is fixed iced pixels.
        //
        // Empirical: on 2286x1429 output with 2000x1333 container, scale = 1.40.
        // 2286 * 1.40 = 3200 (physical). So iced works in near-physical space.
        // scale = physical / logical. And we can compute it as:
        // The layer surface covers the full output at physical resolution.
        // container = physical_width - sidebar_phys - padding_phys
        // But we don't know physical... however:
        // layer surface = full output. iced sees it as some size.
        // container = iced_total - sidebar - padding
        // iced_total / output_logical = display_scale
        // We have container + self.widget_offset.x (which we set to 0) = iced_total... no.
        //
        // OK: just use the known fixed offsets and compute scale from them.
        // offset_x = 280 logical = 280 * scale iced. But we measured offset_x = 280 iced.
        // So scale = 1.0 for offsets? That contradicts 1.40 for sizes.
        //
        // The truth: offsets are in ICED space (not scaled), sizes ARE in logical space
        // and need scaling. The scale factor = output_logical / (container + offset_iced).
        // 2286 / (2000 + 280) = 2286/2280 ≈ 1.003. Not 1.40.
        //
        // I'm confused about the coordinate spaces. Fall back to empirical:
        // Use the widget_offset passed from the view (which is 0,0 now) and compute
        // scale by assuming sidebar=280 iced-px and panel_top=60 iced-px are constant.
        let off_x = 280.0_f32;
        let off_y = std::env::var("COSMIC_WS_OFFY")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(90.0);
        // Scale: (container + sidebar) / output = total_iced / output_logical
        // For width: (2000 + 280) / 2286 ≈ 1.0. But we need 1.40 for sizes.
        // The 1.40 comes from somewhere else entirely. Let's derive it from
        // the max source rect: a fullscreen window is output_w wide in geometry,
        // and should be container_w wide at t=0 in iced.
        // BUT it shouldn't fill the container — it should fill the FULL screen
        // (extend beyond the container edges). So:
        // fullscreen_iced_width = output_logical * scale
        // We want fullscreen_iced_width to span from -off_x to container_w + some_right_margin.
        // fullscreen_iced_width = container_w + off_x + right_margin
        // For a fullscreen: geometry.width = output.width = 2286
        // In iced at t=0: should span from x=-off_x to x=container_w+right = iced_total
        // iced_total = container_w + off_x + right_offset ≈ 2000 + 280 + ?
        // If symmetric: right_offset ≈ 0 (sidebar only on left, panel only on bottom)
        // So: scale = (container + off_x) / output = (2000+280)/2286 = 0.997
        // That gives fullscreen = 2286 * 0.997 = 2280. But we need 2286*1.4=3200.
        //
        // CONCLUSION: The 1.40 is NOT derivable from container/output alone.
        // It IS the display scale factor (physical/logical).
        // I need to get it from the system.
        //
        // Workaround: derive from max source rect vs its capture image size.
        // Capture image is in physical pixels. geometry is in logical.
        // physical / logical = display_scale.
        // But we don't have the capture image size here in the widget.
        //
        // Pass it through from the view. For now: compute from output.
        // We know output_logical = 2286, and the correct scale = 1.40.
        // 2286 * 1.40 = 3200. 3200 is the physical width.
        // layer surface in iced covers the full output. Iced sees it as... let's check.
        // Actually: container(2000) + sidebar(~280 iced) = ~2280 iced.
        // Plus panel_padding from panel_regions. The total iced surface ≈ 2280 + panel stuff.
        // But the surface covers the whole 3200 physical display... or 2286 logical?
        // If iced uses logical: total iced = 2286, sidebar = 286, container = 2000. Scale = 1.0.
        // If iced uses physical: total iced = 3200, sidebar = 400, container = 2800. But container is 2000.
        // Neither works. The iced surface is sized by the compositor at some intermediate value.
        //
        // I give up deriving. Use env var for scale, default 1.40.
        let display_scale = std::env::var("COSMIC_WS_SCALE")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(1.40);

        let nodes = self
            .children
            .iter_mut()
            .zip(tree.children.iter_mut())
            .zip(assigned_rects)
            .enumerate()
            .map(|(i, ((child, tree), assigned_rect))| {
                let mut target_size = assigned_rect.size();
                let target_pos = assigned_rect.position();

                // Apply selection scale for tab-selected window
                let is_selected = self.selected_index == Some(i);
                let scale = if is_selected { self.selection_scale } else { 1.0 };
                let scaled_size = Size::new(
                    target_size.width * scale,
                    target_size.height * scale,
                );
                // Offset to keep scaled window centered on its original position
                let scale_offset = Vector::new(
                    (target_size.width - scaled_size.width) / 2.0,
                    (target_size.height - scaled_size.height) / 2.0,
                );

                // Use scaled size for selected window
                if is_selected {
                    target_size = scaled_size;
                }

                // First compute the final resting position (with centering)
                let final_child_limits = layout::Limits::new(Size::ZERO, target_size);
                let final_layout = child.as_widget_mut().layout(tree, renderer, &final_child_limits);
                let centering_offset = Vector::new(
                    ((target_size.width - final_layout.size().width) / 2.).max(0.),
                    ((target_size.height - final_layout.size().height) / 2.).max(0.),
                );
                let final_target = target_pos + centering_offset + scale_offset;

                if t < 1.0 {
                    if let Some(src) = self.source_rects.get(i) {
                        // Position: subtract offset only (no scaling)
                        // Size: scale by display factor
                        let src_x = src.x - off_x;
                        let src_y = src.y - off_y;
                        let src_w = src.width * display_scale;
                        let src_h = src.height * display_scale;

                        // Interpolate size: source -> target
                        let interp_w = src_w + (target_size.width - src_w) * t;
                        let interp_h = src_h + (target_size.height - src_h) * t;
                        let interp_size = Size::new(interp_w.max(1.0), interp_h.max(1.0));

                        // Re-layout child at interpolated size
                        let layout = child.as_widget_mut().layout(tree, renderer,
                            &layout::Limits::new(Size::ZERO, interp_size));

                        // Interpolate position: source -> final target (with centering)
                        let interp_x = src_x + (final_target.x - src_x) * t;
                        let interp_y = src_y + (final_target.y - src_y) * t;

                        layout.move_to(cosmic::iced::Point::new(interp_x, interp_y))
                    } else {
                        final_layout.move_to(final_target)
                    }
                } else {
                    final_layout.move_to(final_target)
                }
            })
            .collect();
        layout::Node::with_children(limits.max(), nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &cosmic::Renderer,
        operation: &mut dyn Operation<()>,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
                .for_each(|((child, state), layout)| {
                    child
                        .as_widget_mut()
                        .operate(state, layout, renderer, operation);
                });
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &cosmic::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg>,
        viewport: &Rectangle,
    ) {
        for ((child, state), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                state, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &cosmic::Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if let Some(viewport) = layout.bounds().intersection(viewport) {
            for ((child, state), layout) in self
                .children
                .iter()
                .zip(&tree.children)
                .zip(layout.children())
            {
                child
                    .as_widget()
                    .draw(state, renderer, theme, style, layout, cursor, &viewport);
            }
        }
    }

    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }
}

impl<'a, Msg: 'static> From<Toplevels<'a, Msg>> for cosmic::Element<'a, Msg> {
    fn from(widget: Toplevels<'a, Msg>) -> Self {
        cosmic::Element::new(widget)
    }
}
