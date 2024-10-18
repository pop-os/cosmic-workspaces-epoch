use cosmic::iced::{
    advanced::{
        layout::{self, flex::Axis},
        mouse, renderer,
        widget::{Operation, Tree},
        Clipboard, Layout, Shell, Widget,
    },
    event::{self, Event},
    Length, Point, Rectangle, Size,
};
use std::marker::PhantomData;

// Duplicate of private methods
trait AxisExt {
    fn main(&self, size: Size) -> f32;
    fn cross(&self, size: Size) -> f32;
    fn pack(&self, main: f32, cross: f32) -> (f32, f32);
}

impl AxisExt for Axis {
    fn main(&self, size: Size) -> f32 {
        match self {
            Axis::Horizontal => size.width,
            Axis::Vertical => size.height,
        }
    }

    fn cross(&self, size: Size) -> f32 {
        match self {
            Axis::Horizontal => size.height,
            Axis::Vertical => size.width,
        }
    }

    fn pack(&self, main: f32, cross: f32) -> (f32, f32) {
        match self {
            Axis::Horizontal => (main, cross),
            Axis::Vertical => (cross, main),
        }
    }
}

pub fn toplevels<Msg>(children: Vec<cosmic::Element<Msg>>) -> Toplevels<Msg> {
    Toplevels {
        axis: Axis::Horizontal,
        children,
        _msg: PhantomData,
    }
}

pub struct Toplevels<'a, Msg> {
    axis: Axis,
    children: Vec<cosmic::Element<'a, Msg>>,
    _msg: PhantomData<Msg>,
}

impl<'a, Msg> Widget<Msg, cosmic::Theme, cosmic::Renderer> for Toplevels<'a, Msg> {
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            // TODO Make depend on orientation or drop that option
            height: Length::Shrink,
        }
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // TODO configurable
        let spacing = 16;

        // Get total requested main axis length if widget could have all the space
        let total_spacing = spacing * (self.children.len().saturating_sub(1)).max(0);
        let requested_mains = self
            .children
            .iter()
            .zip(tree.children.iter_mut())
            .map(|(child, tree)| {
                let child_limits = layout::Limits::new(Size::ZERO, limits.max());
                let layout = child.as_widget().layout(tree, renderer, &child_limits);
                self.axis.main(layout.size())
            })
            .collect::<Vec<_>>();
        let requested_main_total: f32 = requested_mains.iter().sum::<f32>() + total_spacing as f32;

        let scale_factor = (self.axis.main(limits.max()) / requested_main_total).min(1.0);

        let max_cross = self.axis.cross(limits.max());

        // XXX sill allocating maximum main axis?
        // - what was it doing before?
        let mut total_main = 0.0;
        let mut first = true;
        let nodes = self
            .children
            .iter()
            .zip(tree.children.iter_mut())
            .zip(requested_mains.iter())
            .map(|((child, tree), requested_main)| {
                if !first {
                    total_main += spacing as f32;
                }
                first = false;

                let max_main = requested_main * scale_factor;

                let (max_width, max_height) = self.axis.pack(max_main, max_cross);
                let child_limits =
                    layout::Limits::new(Size::ZERO, Size::new(max_width, max_height));
                let mut layout = child.as_widget().layout(tree, renderer, &child_limits);
                // Center on cross axis
                let cross = ((max_cross - self.axis.cross(layout.size())) / 2.).max(0.);
                let (x, y) = self.axis.pack(total_main, cross);
                layout = layout.move_to(Point::new(x, y));
                total_main += self.axis.main(layout.size());
                layout
            })
            .collect();

        let (total_width, total_height) = self.axis.pack(total_main, max_cross);
        let size = Size::new(total_width, total_height);
        layout::Node::with_children(size, nodes)
    }

    fn operate(
        &self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &cosmic::Renderer,
        operation: &mut dyn Operation<()>,
    ) {
        operation.container(None, layout.bounds(), &mut |operation| {
            self.children
                .iter()
                .zip(&mut tree.children)
                .zip(layout.children())
                .for_each(|((child, state), layout)| {
                    child
                        .as_widget()
                        .operate(state, layout, renderer, operation);
                });
        });
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &cosmic::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg>,
        viewport: &Rectangle,
    ) -> event::Status {
        self.children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child.as_widget_mut().on_event(
                    state,
                    event.clone(),
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                )
            })
            .fold(event::Status::Ignored, event::Status::merge)
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
