//! If `visible` is set to `true`, behaves exactly as wrapped widget. If `false`,
//! takes the same space but does not draw.

use cosmic::iced::{
    advanced::{
        layout, mouse, renderer,
        widget::{Operation, Tree},
        Clipboard, Layout, Shell, Widget,
    },
    event::{self, Event},
    Length, Rectangle, Size,
};
use std::marker::PhantomData;

pub fn visibility_wrapper<'a, Msg, T: Into<cosmic::Element<'a, Msg>>>(
    inner: T,
    visible: bool,
) -> VisibilityWrapper<'a, Msg> {
    VisibilityWrapper {
        content: inner.into(),
        visible,
        _msg: PhantomData,
    }
}

pub struct VisibilityWrapper<'a, Msg> {
    content: cosmic::Element<'a, Msg>,
    visible: bool,
    _msg: PhantomData<Msg>,
}

impl<'a, Msg> Widget<Msg, cosmic::Theme, cosmic::Renderer> for VisibilityWrapper<'a, Msg> {
    delegate::delegate! {
        to self.content.as_widget() {
            fn size(&self) -> Size<Length>;
            fn size_hint(&self) -> Size<Length>;
        }
    }

    fn operate(
        &self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &cosmic::Renderer,
        operation: &mut dyn Operation<()>,
    ) {
        self.content
            .as_widget()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        self.content.as_widget_mut().on_event(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        )
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &cosmic::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget()
            .layout(&mut tree.children[0], renderer, limits)
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
        if self.visible {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout,
                cursor,
                viewport,
            );
        }
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut [&mut self.content]);
    }
}

impl<'a, Msg: 'a> From<VisibilityWrapper<'a, Msg>> for cosmic::Element<'a, Msg> {
    fn from(widget: VisibilityWrapper<'a, Msg>) -> Self {
        cosmic::Element::new(widget)
    }
}
