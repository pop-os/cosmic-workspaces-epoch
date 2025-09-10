use aliasable::vec::AliasableVec;
use cosmic::iced::{Length, Point, Rectangle, Size, advanced::layout::flex::Axis};
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
        toplevels: &[LayoutToplevel<'_, AxisSize>],
    ) -> impl Iterator<Item = AxisRectangle>;
}

impl<T: AxisToplevelLayout> ToplevelLayout for T {
    fn size(&self) -> Size<Length> {
        self.size().pack(self.axis())
    }

    fn layout<'a>(
        &self,
        max_limit: Size,
        toplevels: &[LayoutToplevel<'a>],
    ) -> impl Iterator<Item = Rectangle> {
        let max_limit = AxisSize::unpack(self.axis(), max_limit);
        let toplevels = toplevels
            .iter()
            .map(|t| LayoutToplevel {
                preferred_size: AxisSize::unpack(self.axis(), t.preferred_size),
                _phantom_data: PhantomData,
            })
            .collect::<Vec<_>>();
        let toplevels = AliasableVec::from_unique(toplevels);
        // Extend lifetime
        let toplevels_slice = unsafe {
            std::mem::transmute::<&[LayoutToplevel<'_, AxisSize>], &'a [LayoutToplevel<'a, AxisSize>]>(
                &*toplevels,
            )
        };
        let inner = self
            .layout(max_limit, toplevels_slice)
            .map(|rect| rect.pack(self.axis()));
        AxisLayoutIterator {
            inner,
            _toplevels: toplevels,
        }
    }
}

struct AxisLayoutIterator<'a, I: Iterator<Item = Rectangle>> {
    inner: I,
    // After `inner` so it is dropped only after that is dropped
    _toplevels: AliasableVec<LayoutToplevel<'a, AxisSize>>,
}

impl<I: Iterator<Item = Rectangle>> Iterator for AxisLayoutIterator<'_, I> {
    type Item = Rectangle;

    fn next(&mut self) -> Option<Rectangle> {
        self.inner.next()
    }
}
