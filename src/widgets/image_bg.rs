use cosmic::iced::{
    self,
    advanced::{
        layout::{self, flex::Axis},
        mouse, overlay, renderer,
        widget::{tree, Operation, OperationOutputWrapper, Tree},
        Clipboard, Layout, Shell, Widget,
    },
    event::{self, Event},
    widget::image::{FilterMethod, Handle},
    ContentFit, Length, Point, Rectangle, Size, Vector,
};
use cosmic::iced_core::Renderer as _;

use std::marker::PhantomData;

pub fn image_bg<'a, Msg, T: Into<cosmic::Element<'a, Msg>>>(content: T) -> ImageBg<'a, Msg> {
    ImageBg {
        content: content.into(),
        _msg: PhantomData,
    }
}

pub struct ImageBg<'a, Msg> {
    content: cosmic::Element<'a, Msg>,
    _msg: PhantomData<Msg>,
}

impl<'a, Msg> Widget<Msg, cosmic::Renderer> for ImageBg<'a, Msg> {
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&mut self, tree: &mut Tree) {
        self.content.as_widget_mut().diff(tree);
    }

    fn width(&self) -> Length {
        self.content.as_widget().width()
    }

    fn height(&self) -> Length {
        self.content.as_widget().height()
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let content = self.content.as_widget().layout(tree, renderer, limits);
        //let size = limits.resolve(content.size());
        let size = content.size();
        layout::Node::with_children(size, vec![content])
    }

    fn operate(
        &self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &cosmic::Renderer,
        operation: &mut dyn Operation<OperationOutputWrapper<Msg>>,
    ) {
        operation.container(
            None, // XXX id
            layout.bounds(),
            &mut |operation| {
                self.content.as_widget().operate(
                    tree,
                    layout.children().next().unwrap(),
                    renderer,
                    operation,
                );
            },
        );
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
            tree,
            event,
            layout.children().next().unwrap(),
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
            tree,
            layout.children().next().unwrap(),
            cursor,
            viewport,
            renderer,
        )
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
        use cosmic::iced_core::image::Renderer;

        // TODO desktop background?
        let handle =
            Handle::from_path("/usr/share/backgrounds/pop/kate-hazen-COSMIC-desktop-wallpaper.png");

        let Size { width, height } = renderer.dimensions(&handle);
        let image_size = Size::new(width as f32, height as f32);

        let bounds = layout.bounds();
        let adjusted_fit = ContentFit::Cover.fit(image_size, bounds.size());

        let offset = Vector::new(
            (bounds.width - adjusted_fit.width).max(0.0) / 2.0,
            (bounds.height - adjusted_fit.height).max(0.0) / 2.0,
        );

        let drawing_bounds = Rectangle {
            width: adjusted_fit.width,
            height: adjusted_fit.height,
            ..bounds
        };

        // layer?
        //renderer.with_layer(bounds, |renderer| {
        renderer.draw(
            handle.clone(),
            FilterMethod::default(),
            drawing_bounds + offset,
            [0.0, 0.0, 0.0, 0.0],
        );
        //});

        self.content.draw(
            state,
            renderer,
            theme,
            style,
            layout.children().next().unwrap(),
            cursor,
            viewport,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &cosmic::Renderer,
    ) -> Option<overlay::Element<'b, Msg, cosmic::Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout.children().next().unwrap(), renderer)
    }
}

impl<'a, Msg: 'static> From<ImageBg<'a, Msg>> for cosmic::Element<'a, Msg> {
    fn from(widget: ImageBg<'a, Msg>) -> Self {
        cosmic::Element::new(widget)
    }
}
