use cosmic::iced::{advanced::layout::flex::Axis, Size};

// Duplicate of private methods
pub(super) trait AxisExt {
    fn main(&self, size: Size) -> f32;
    fn cross(&self, size: Size) -> f32;
    fn pack(&self, main: f32, cross: f32) -> (f32, f32);
}

impl AxisExt for Axis {
    fn main(&self, size: Size) -> f32 {
        match self {
            Axis::Horizontal => size.width,
            Axis::Vertical => size.height,
        }
    }

    fn cross(&self, size: Size) -> f32 {
        match self {
            Axis::Horizontal => size.height,
            Axis::Vertical => size.width,
        }
    }

    fn pack(&self, main: f32, cross: f32) -> (f32, f32) {
        match self {
            Axis::Horizontal => (main, cross),
            Axis::Vertical => (cross, main),
        }
    }
}
