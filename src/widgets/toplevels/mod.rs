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

pub fn toplevels<Msg>(children: Vec<cosmic::Element<Msg>>) -> Toplevels<Msg> {
    Toplevels {
        // TODO configurable
        layout: TwoRowColToplevelLayout::new(Axis::Horizontal, 16),
        children,
        _msg: PhantomData,
    }
}

pub struct Toplevels<'a, Msg> {
    layout: TwoRowColToplevelLayout,
    children: Vec<cosmic::Element<'a, Msg>>,
    _msg: PhantomData<Msg>,
}

impl<Msg> Widget<Msg, cosmic::Theme, cosmic::Renderer> for Toplevels<'_, Msg> {
    fn size(&self) -> Size<Length> {
        self.layout.size()
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // Call `.layout()` on each child with full limits to determine "preferred" sizes
        let layout_toplevels = self
            .children
            .iter()
            .zip(tree.children.iter_mut())
            .map(|(child, tree)| {
                let preferred_size = child.as_widget().layout(tree, renderer, limits).size();
                LayoutToplevel {
                    preferred_size,
                    _phantom_data: PhantomData,
                }
            })
            .collect::<Vec<_>>();

        // Assign rectangles for each child using `ToplevelLayout` backend
        let assigned_rects = self.layout.layout(limits.max(), &layout_toplevels);

        let nodes = self
            .children
            .iter()
            .zip(tree.children.iter_mut())
            .zip(assigned_rects)
            .map(|((child, tree), assigned_rect)| {
                let child_limits = layout::Limits::new(Size::ZERO, assigned_rect.size());
                let layout = child.as_widget().layout(tree, renderer, &child_limits);

                // Center on both axes, if child didn't consume full size allocation
                let centering_offset = Vector::new(
                    ((assigned_rect.size().width - layout.size().width) / 2.).max(0.),
                    ((assigned_rect.size().height - layout.size().height) / 2.).max(0.),
                );

                layout.move_to(assigned_rect.position() + centering_offset)
            })
            .collect();
        layout::Node::with_children(limits.max(), nodes)
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
