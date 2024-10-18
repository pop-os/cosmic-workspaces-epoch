// Renders image behind widget, and otherwise passes through all behavior

use cosmic::{
    iced::{
        advanced::{
            layout::{self},
            mouse, overlay, renderer,
            widget::{tree, Operation, Tree},
            Clipboard, Layout, Shell, Widget,
        },
        event::{self, Event},
        Length, Rectangle, Size, Vector,
    },
    iced_core::Renderer,
};

use std::marker::PhantomData;

pub fn image_bg<'a, Msg, T1: Into<cosmic::Element<'a, Msg>>, T2: Into<cosmic::Element<'a, Msg>>>(
    content: T1,
    bg: T2,
) -> ImageBg<'a, Msg> {
    ImageBg {
        content: content.into(),
        bg: bg.into(),
        _msg: PhantomData,
    }
}

pub struct ImageBg<'a, Msg> {
    content: cosmic::Element<'a, Msg>,
    bg: cosmic::Element<'a, Msg>,
    _msg: PhantomData<Msg>,
}

impl<'a, Msg> Widget<Msg, cosmic::Theme, cosmic::Renderer> for ImageBg<'a, Msg> {
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
                    operation: &mut dyn Operation<()>,
                );
            fn mouse_interaction(
                &self,
                tree: &Tree,
                layout: Layout<'_>,
                cursor: mouse::Cursor,
                viewport: &Rectangle,
                renderer: &cosmic::Renderer,
            ) -> mouse::Interaction;
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
                translation: Vector,
            ) -> Option<overlay::Element<'b, Msg, cosmic::Theme, cosmic::Renderer>>;
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
        self.bg
            .as_widget()
            .draw(state, renderer, theme, style, layout, cursor, viewport);

        let bounds = layout.bounds();
        renderer.with_layer(bounds, |renderer| {
            self.content
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport)
        });
    }
}

impl<'a, Msg: 'static> From<ImageBg<'a, Msg>> for cosmic::Element<'a, Msg> {
    fn from(widget: ImageBg<'a, Msg>) -> Self {
        cosmic::Element::new(widget)
    }
}
