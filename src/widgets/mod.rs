use cosmic::iced::{
    advanced::{
        layout, mouse, overlay, renderer,
        widget::{tree, Id, Operation, Tree},
        Clipboard, Layout, Shell, Widget,
    },
    event::{self, Event},
    Length, Rectangle, Size, Vector,
};
use std::marker::PhantomData;

mod image_bg;
pub use image_bg::image_bg;
mod workspace_bar;
pub use workspace_bar::workspace_bar;
mod toplevel_item;
pub use toplevel_item::toplevel_item;
mod mouse_interaction_wrapper;
pub use mouse_interaction_wrapper::mouse_interaction_wrapper;
mod toplevels;
pub use toplevels::toplevels;
mod visibility_wrapper;
pub use visibility_wrapper::visibility_wrapper;

// Widget for debugging
#[allow(dead_code)]
pub fn layout_wrapper<'a, Msg, T: Into<cosmic::Element<'a, Msg>>>(
    inner: T,
) -> LayoutWrapper<'a, Msg> {
    LayoutWrapper {
        content: inner.into(),
        _msg: PhantomData,
    }
}

pub struct LayoutWrapper<'a, Msg> {
    content: cosmic::Element<'a, Msg>,
    _msg: PhantomData<Msg>,
}

impl<'a, Msg> Widget<Msg, cosmic::Theme, cosmic::Renderer> for LayoutWrapper<'a, Msg> {
    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        dbg!(limits);
        dbg!(self.content.as_widget().layout(tree, renderer, limits))
    }

    delegate::delegate! {
        to self.content.as_widget() {
            fn tag(&self) -> tree::Tag;
            fn state(&self) -> tree::State;
            fn children(&self) -> Vec<Tree>;
            fn size(&self) -> Size<Length>;
            fn size_hint(&self) -> Size<Length>;
            fn operate(
                    &self,
                    tree: &mut Tree,
                    layout: Layout<'_>,
                    renderer: &cosmic::Renderer,
                    operation: &mut dyn Operation<()>,
                );
            fn draw(
                &self,
                state: &Tree,
                renderer: &mut cosmic::Renderer,
                theme: &cosmic::Theme,
                style: &renderer::Style,
                layout: Layout<'_>,
                cursor: mouse::Cursor,
                viewport: &Rectangle,
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
                transation: Vector,
            ) -> Option<overlay::Element<'b, Msg, cosmic::Theme, cosmic::Renderer>>;
            fn set_id(&mut self, id: Id);
        }
    }
}

impl<'a, Msg: 'a> From<LayoutWrapper<'a, Msg>> for cosmic::Element<'a, Msg> {
    fn from(widget: LayoutWrapper<'a, Msg>) -> Self {
        cosmic::Element::new(widget)
    }
}
