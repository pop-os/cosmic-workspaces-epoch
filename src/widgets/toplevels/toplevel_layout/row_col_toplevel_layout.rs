use cosmic::iced::{Length, advanced::layout::flex::Axis};

use super::{
    LayoutToplevel,
    axis_toplevel_layout::{AxisPoint, AxisRectangle, AxisSize, AxisToplevelLayout},
};

pub(crate) struct RowColToplevelLayout {
    pub axis: Axis,
    pub spacing: u32,
}

impl RowColToplevelLayout {
    pub fn new(axis: Axis, spacing: u32) -> Self {
        Self { axis, spacing }
    }

    // Get total requested main axis length if widget could have all the space
    pub fn requested_main_total(&self, toplevels: &[LayoutToplevel<'_, AxisSize>]) -> f32 {
        self.requested_main_total_without_spacing(toplevels) + self.total_spacing(toplevels)
    }

    fn requested_main_total_without_spacing(
        &self,
        toplevels: &[LayoutToplevel<'_, AxisSize>],
    ) -> f32 {
        toplevels.iter().map(|t| t.preferred_size.main).sum::<f32>()
    }

    fn total_spacing(&self, toplevels: &[LayoutToplevel<'_, AxisSize>]) -> f32 {
        (self.spacing as usize * toplevels.len().saturating_sub(1)).max(0) as f32
    }

    pub fn requested_cross_max(&self, toplevels: &[LayoutToplevel<'_, AxisSize>]) -> f32 {
        toplevels
            .iter()
            .map(|t| t.preferred_size.cross)
            .max_by(f32::total_cmp)
            .unwrap_or(1.0)
    }

    pub fn scale_factor(
        &self,
        max_limit: AxisSize,
        toplevels: &[LayoutToplevel<'_, AxisSize>],
    ) -> f32 {
        let scale_factor_main = (max_limit.main - self.total_spacing(toplevels))
            / self.requested_main_total_without_spacing(toplevels);
        let scale_factor_cross = max_limit.cross / self.requested_cross_max(toplevels);
        scale_factor_main.min(scale_factor_cross).min(1.)
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
        toplevels: &[LayoutToplevel<'_, AxisSize>],
    ) -> impl Iterator<Item = AxisRectangle> {
        let scale_factor = self.scale_factor(max_limit, toplevels);

        // Add padding to center if total requested size doesn't fill available space
        let padding = (max_limit.main
            - scale_factor * self.requested_main_total_without_spacing(toplevels)
            - self.total_spacing(toplevels))
        .max(0.)
            / 2.;

        let mut total_main = padding;
        let mut first = true;
        toplevels.iter().map(move |child| {
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
