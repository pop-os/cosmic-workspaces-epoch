use cosmic::iced::{Length, advanced::layout::flex::Axis};

use super::{
    LayoutToplevel,
    axis_toplevel_layout::{AxisRectangle, AxisSize, AxisToplevelLayout},
    row_col_toplevel_layout::RowColToplevelLayout,
};

pub(crate) struct TwoRowColToplevelLayout(RowColToplevelLayout);

impl TwoRowColToplevelLayout {
    pub fn new(axis: Axis, spacing: u32) -> Self {
        Self(RowColToplevelLayout::new(axis, spacing))
    }
}

impl AxisToplevelLayout for TwoRowColToplevelLayout {
    fn axis(&self) -> &Axis {
        &self.0.axis
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
        let requested_main_total = self.0.requested_main_total(toplevels);
        let scale_factor = self.0.scale_factor(max_limit, toplevels);

        let half_max_limit = AxisSize {
            main: max_limit.main,
            cross: (max_limit.cross - self.0.spacing as f32) / 2.,
        };

        // See if two row layout is better
        // TODO not a good fix if there is a large window and many smaller ones?
        if requested_main_total > max_limit.main && toplevels.len() > 1 {
            // decide best way to partition list
            let (split_point, two_row_scale_factor) = (1..toplevels.len())
                .map(|i| {
                    let (top_row, bottom_row) = toplevels.split_at(i);
                    let top_scale_factor = self.0.scale_factor(half_max_limit, top_row);
                    let bottom_scale_factor = self.0.scale_factor(half_max_limit, bottom_row);
                    (i, top_scale_factor.min(bottom_scale_factor))
                })
                .max_by(|(_, scale1), (_, scale2)| scale1.total_cmp(scale2))
                .unwrap();
            // Better layout
            if two_row_scale_factor > scale_factor {
                // TODO padding
                let row1 = self.0.layout(half_max_limit, &toplevels[..split_point]);
                let row2 = self
                    .0
                    .layout(half_max_limit, &toplevels[split_point..])
                    .map(move |mut rect| {
                        rect.origin.cross += half_max_limit.cross + self.0.spacing as f32;
                        rect
                    });
                return itertools::Either::Left(row1.chain(row2));
            }
        }

        itertools::Either::Right(self.0.layout(max_limit, toplevels))
    }
}
