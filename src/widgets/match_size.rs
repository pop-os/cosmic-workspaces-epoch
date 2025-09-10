//! Show one surface, sized to match the size of another (invisible) widget

use cosmic::iced::{
    Length, Rectangle, Size,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Operation, Tree},
    },
    event::{self, Event},
};
use std::marker::PhantomData;

pub fn match_size<
    'a,
    Msg,
    T1: Into<cosmic::Element<'a, Msg>>,
    T2: Into<cosmic::Element<'a, Msg>>,
>(
    matched: T1,
    shown: T2,
) -> MatchSize<'a, Msg> {
    MatchSize {
        matched: matched.into(),
        shown: shown.into(),
        _msg: PhantomData,
    }
}

pub struct MatchSize<'a, Msg> {
    matched: cosmic::Element<'a, Msg>,
    shown: cosmic::Element<'a, Msg>,
    _msg: PhantomData<Msg>,
}

impl<Msg> Widget<Msg, cosmic::Theme, cosmic::Renderer> for MatchSize<'_, Msg> {
    delegate::delegate! {
        to self.matched.as_widget() {
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
        self.matched
            .as_widget()
            .operate(&mut tree.children[0], layout, renderer, operation);
        self.shown
            .as_widget()
            .operate(&mut tree.children[1], layout, renderer, operation);
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
        self.shown.as_widget_mut().on_event(
            &mut tree.children[1],
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
        self.shown.as_widget().mouse_interaction(
            &tree.children[1],
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
        // TODO?
        self.matched
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
        self.shown.as_widget().draw(
            &tree.children[1],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.matched), Tree::new(&self.shown)]
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut [&mut self.matched, &mut self.shown]);
    }
}

impl<'a, Msg: 'a> From<MatchSize<'a, Msg>> for cosmic::Element<'a, Msg> {
    fn from(widget: MatchSize<'a, Msg>) -> Self {
        cosmic::Element::new(widget)
    }
}
