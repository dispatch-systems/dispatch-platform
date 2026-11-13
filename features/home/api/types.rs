//! A member's Home page layout, written to TypeScript in `generated/`.
use serde::{Deserialize, Serialize};

/// Where a member placed each widget on their Home page, in cells of its grid: twelve columns,
/// and rows each as tall as a column is wide. A widget of a feature they don't have now keeps
/// its place for when it returns.
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/home/api/generated/"))]
#[serde(deny_unknown_fields)]
pub struct HomeLayout {
    pub widgets: Vec<PlacedWidget>,
}

/// One widget, by its id, at column `x` and row `y` from the top left, `w` columns wide and `h`
/// rows tall.
#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/home/api/generated/"))]
#[serde(deny_unknown_fields)]
pub struct PlacedWidget {
    pub id: String,
    pub x: u8,
    pub y: u16,
    pub w: u8,
    pub h: u8,
}
