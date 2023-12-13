use cosmic::iced::{
    self,
    advanced::{layout, mouse, renderer, widget::Tree, Layout, Widget},
    Length, Rectangle,
};
use std::marker::PhantomData;

mod workspace_bar;
pub use workspace_bar::workspace_bar;

pub fn layout_wrapper<Msg, T: Widget<Msg, cosmic::Renderer>>(inner: T) -> LayoutWrapper<Msg, T> {
    LayoutWrapper {
        inner,
        _msg: PhantomData,
    }
}

pub struct LayoutWrapper<Msg, T: Widget<Msg, cosmic::Renderer>> {
    inner: T,
    _msg: PhantomData<Msg>,
}

impl<Msg, T: Widget<Msg, cosmic::Renderer>> Widget<Msg, cosmic::Renderer>
    for LayoutWrapper<Msg, T>
{
    fn width(&self) -> Length {
        self.inner.width()
    }

    fn height(&self) -> Length {
        self.inner.height()
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        dbg!(limits);
        dbg!(self.inner.layout(tree, renderer, limits))
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
        self.inner
            .draw(state, renderer, theme, style, layout, cursor, viewport)
    }

    fn children(&self) -> Vec<Tree> {
        self.inner.children()
    }
}

impl<'a, Msg: 'a, T: Widget<Msg, cosmic::Renderer> + 'a> From<LayoutWrapper<Msg, T>>
    for cosmic::Element<'a, Msg>
{
    fn from(widget: LayoutWrapper<Msg, T>) -> Self {
        cosmic::Element::new(widget)
    }
}
