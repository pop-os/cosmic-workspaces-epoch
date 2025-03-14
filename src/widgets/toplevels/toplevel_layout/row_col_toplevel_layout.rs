use cosmic::iced::{advanced::layout::flex::Axis, Length};

use super::{
    axis_toplevel_layout::{AxisPoint, AxisRectangle, AxisSize, AxisToplevelLayout},
    LayoutToplevel,
};

pub(crate) struct RowColToplevelLayout {
    pub axis: Axis,
    pub spacing: u32,
}

impl RowColToplevelLayout {
    // Get total requested main axis length if widget could have all the space
    fn requested_main_total(&self, toplevels: &[LayoutToplevel<'_, AxisSize>]) -> f32 {
        let total_spacing = self.spacing as usize * (toplevels.len().saturating_sub(1)).max(0);
        toplevels.iter().map(|t| t.preferred_size.main).sum::<f32>() + total_spacing as f32
    }
}

impl AxisToplevelLayout for RowColToplevelLayout {
    fn axis(&self) -> &Axis {
        &self.axis
    }

    fn size(&self) -> AxisSize<Length> {
        AxisSize {
            main: Length::Fill,
            cross: Length::Shrink,
        }
    }

    fn layout(
        &self,
        max_limit: AxisSize,
        toplevels: Vec<LayoutToplevel<'_, AxisSize>>,
    ) -> impl Iterator<Item = AxisRectangle> {
        let requested_main_total = self.requested_main_total(&toplevels);
        let scale_factor = (max_limit.main / requested_main_total).min(1.0);

        // Add padding to center if total requested size doesn't fill available space
        let padding = (max_limit.main - requested_main_total).max(0.) / 2.;

        let mut total_main = padding;
        let mut first = true;
        toplevels.into_iter().map(move |child| {
            if !first {
                total_main += self.spacing as f32;
            }
            first = false;

            let max_main = child.preferred_size.main * scale_factor;

            let main = total_main;
            total_main += max_main;

            AxisRectangle::new(
                AxisPoint { main, cross: 0.0 },
                AxisSize {
                    main: max_main,
                    cross: max_limit.cross,
                },
            )
        })
    }
}
