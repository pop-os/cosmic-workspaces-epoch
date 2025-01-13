// TODO: More generic widget in libcosmic? Improve iced layout system?
// - preferred_size concept

use cosmic::iced::{Length, Rectangle, Size};
use std::marker::PhantomData;

mod row_col_toplevel_layout;
mod utils;
pub(crate) use row_col_toplevel_layout::RowColToplevelLayout;

pub(crate) struct LayoutToplevel<'a> {
    //toplevel: &'a crate::Toplevel,
    /// Preferred size of the child widget, if it fill the parent container
    pub preferred_size: Size,
    pub _phantom_data: PhantomData<&'a crate::Toplevel>,
}

/// An implementor of this trait defines a layout for the [`Toplevels`] widget
/// as a pure function, without dealing with all the details of the iced layout
/// system.
pub(crate) trait ToplevelLayout {
    /// [`Size`] the container widget should request
    fn size(&self) -> Size<Length>;
    /// Decide size and location of each widget
    ///
    /// - `max_limit` is the total size available for all children
    /// - For each entry in `toplevels`, this should yield one `Rectangle`
    ///
    /// If a child doesn't use it's entire rectangle, it will be centered in that space.
    fn layout(
        &self,
        max_limit: Size,
        toplevels: &[LayoutToplevel<'_>],
    ) -> impl Iterator<Item = Rectangle>;
}
