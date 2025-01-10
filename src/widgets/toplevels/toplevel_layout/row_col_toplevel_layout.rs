use cosmic::iced::{advanced::layout::flex::Axis, Length, Point, Rectangle, Size};

use super::{utils::AxisExt, LayoutToplevel, ToplevelLayout};

pub(crate) struct RowColToplevelLayout {
    pub axis: Axis,
    pub spacing: u32,
}

impl ToplevelLayout for RowColToplevelLayout {
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            // TODO Make depend on orientation or drop that option
            height: Length::Shrink,
        }
    }

    fn layout(
        &self,
        max_limit: Size,
        toplevels: &[LayoutToplevel<'_>],
    ) -> impl Iterator<Item = Rectangle> {
        // Get total requested main axis length if widget could have all the space
        let total_spacing = self.spacing as usize * (toplevels.len().saturating_sub(1)).max(0);
        let requested_main_total: f32 = toplevels
            .iter()
            .map(|t| self.axis.main(t.preferred_size))
            .sum::<f32>()
            + total_spacing as f32;
        let scale_factor = (self.axis.main(max_limit) / requested_main_total).min(1.0);
        let max_cross = self.axis.cross(max_limit);

        let mut total_main = 0.0;
        let mut first = true;
        toplevels.iter().map(move |child| {
            let requested_main = self.axis.main(child.preferred_size);
            if !first {
                total_main += self.spacing as f32;
            }
            first = false;

            let max_main = requested_main * scale_factor;

            let (width, height) = self.axis.pack(max_main, max_cross);
            let (x, y) = self.axis.pack(total_main, 0.0);
            total_main += max_main;
            Rectangle::new(Point::new(x, y), Size::new(width, height))
        })
    }
}
