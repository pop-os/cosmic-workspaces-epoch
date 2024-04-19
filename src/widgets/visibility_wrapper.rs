//! If `visible` is set to `true`, behaves exactly as wrapped widget. If `false`,
//! takes the same space but does not draw.

use cosmic::iced::{
    advanced::{
        layout, mouse, overlay, renderer,
        widget::{tree, Id, Operation, OperationOutputWrapper, Tree},
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
            fn tag(&self) -> tree::Tag;
            fn state(&self) -> tree::State;
            fn children(&self) -> Vec<Tree>;
            fn size(&self) -> Size<Length>;
            fn size_hint(&self) -> Size<Length>;
            fn layout(
                    &self,
                    tree: &mut Tree,
                    renderer: &cosmic::Renderer,
                    limits: &layout::Limits,
                ) -> layout::Node;
            fn operate(
                    &self,
                    tree: &mut Tree,
                    layout: Layout<'_>,
                    renderer: &cosmic::Renderer,
                    operation: &mut dyn Operation<OperationOutputWrapper<Msg>>,
                );
            fn mouse_interaction(
                &self,
                _tree: &Tree,
                _layout: Layout<'_>,
                _cursor: mouse::Cursor,
                _viewport: &Rectangle,
                _renderer: &cosmic::Renderer,
            ) -> mouse::Interaction;
            fn id(&self) -> Option<Id>;
        }

        to self.content.as_widget_mut() {
            fn diff(&mut self, tree: &mut Tree);
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
            ) -> event::Status;
            fn overlay<'b>(
                &'b mut self,
                tree: &'b mut Tree,
                layout: Layout<'_>,
                renderer: &cosmic::Renderer,
            ) -> Option<overlay::Element<'b, Msg, cosmic::Theme, cosmic::Renderer>>;
            fn set_id(&mut self, id: Id);
        }
    }

    fn draw(
        &self,
        state: &Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if self.visible {
            self.content
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
        }
    }
}

impl<'a, Msg: 'a> From<VisibilityWrapper<'a, Msg>> for cosmic::Element<'a, Msg> {
    fn from(widget: VisibilityWrapper<'a, Msg>) -> Self {
        cosmic::Element::new(widget)
    }
}
