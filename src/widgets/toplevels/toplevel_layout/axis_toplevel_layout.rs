use cosmic::iced::{advanced::layout::flex::Axis, Length, Point, Rectangle, Size};
use std::marker::PhantomData;

use super::{LayoutToplevel, ToplevelLayout};

#[derive(Debug, Copy, Clone)]
pub struct AxisPoint {
    pub main: f32,
    pub cross: f32,
}

impl AxisPoint {
    fn pack(self, axis: &Axis) -> Point {
        match axis {
            Axis::Horizontal => Point::new(self.main, self.cross),
            Axis::Vertical => Point::new(self.cross, self.main),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct AxisSize<T = f32> {
    pub main: T,
    pub cross: T,
}

impl<T> AxisSize<T> {
    fn unpack(axis: &Axis, size: Size<T>) -> Self {
        match axis {
            Axis::Horizontal => AxisSize {
                main: size.width,
                cross: size.height,
            },
            Axis::Vertical => AxisSize {
                main: size.height,
                cross: size.width,
            },
        }
    }

    fn pack(self, axis: &Axis) -> Size<T> {
        match axis {
            Axis::Horizontal => Size::new(self.main, self.cross),
            Axis::Vertical => Size::new(self.cross, self.main),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct AxisRectangle {
    pub origin: AxisPoint,
    pub size: AxisSize,
}

impl AxisRectangle {
    pub fn new(origin: AxisPoint, size: AxisSize) -> Self {
        Self { origin, size }
    }

    fn pack(self, axis: &Axis) -> Rectangle {
        Rectangle::new(self.origin.pack(axis), self.size.pack(axis))
    }
}

/// Helper to implement [`ToplevelLayout`] for layouts based on an `[Axis]`,
/// that care only about main vs cross, and not width/height.
pub trait AxisToplevelLayout {
    fn axis(&self) -> &Axis;
    fn size(&self) -> AxisSize<Length>;
    fn layout(
        &self,
        max_limit: AxisSize,
        toplevels: Vec<LayoutToplevel<'_, AxisSize>>,
    ) -> impl Iterator<Item = AxisRectangle>;
}

impl<T: AxisToplevelLayout> ToplevelLayout for T {
    fn size(&self) -> Size<Length> {
        self.size().pack(self.axis())
    }

    fn layout(
        &self,
        max_limit: Size,
        toplevels: &[LayoutToplevel<'_>],
    ) -> impl Iterator<Item = Rectangle> {
        let max_limit = AxisSize::unpack(self.axis(), max_limit);
        let toplevels = toplevels
            .into_iter()
            .map(|t| LayoutToplevel {
                preferred_size: AxisSize::unpack(self.axis(), t.preferred_size),
                _phantom_data: PhantomData,
            })
            .collect::<Vec<_>>();
        self.layout(max_limit, toplevels)
            .map(|rect| rect.pack(self.axis()))
    }
}
