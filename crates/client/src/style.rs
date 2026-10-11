//! Tactical display palette (DESIGN.md §10.1): dark, low saturation; affiliation colours
//! desaturated; colour is never the only cue (frame shapes differ too).

use crate::render::Rgba;

pub const BG: Rgba = Rgba(10, 14, 16, 255);
pub const PANEL: Rgba = Rgba(14, 20, 22, 235);
pub const PANEL_EDGE: Rgba = Rgba(60, 80, 70, 255);
pub const TEXT: Rgba = Rgba(170, 196, 168, 255);
pub const TEXT_DIM: Rgba = Rgba(110, 130, 112, 255);
pub const AMBER: Rgba = Rgba(214, 170, 90, 255);

pub const CONTOUR_INDEX: Rgba = Rgba(150, 140, 104, 200);
pub const CONTOUR: Rgba = Rgba(120, 116, 90, 140);
pub const CONTOUR_SUPP: Rgba = Rgba(110, 106, 84, 90);
pub const GRID: Rgba = Rgba(120, 150, 130, 70);
pub const GRID_LABEL: Rgba = Rgba(140, 170, 150, 200);
pub const WATER: Rgba = Rgba(90, 130, 160, 230);
pub const ROAD: Rgba = Rgba(186, 176, 150, 220);
pub const ROAD_FILL: Rgba = Rgba(40, 40, 36, 255);
pub const TRACK: Rgba = Rgba(150, 140, 116, 200);
pub const BUILDING: Rgba = Rgba(58, 58, 56, 255);
pub const BUILDING_EDGE: Rgba = Rgba(150, 146, 132, 220);
pub const FOREST_EDGE: Rgba = Rgba(110, 150, 110, 200);

pub const FRIENDLY: Rgba = Rgba(110, 170, 210, 255);
pub const HOSTILE: Rgba = Rgba(210, 110, 100, 255);
pub const ROUTE: Rgba = Rgba(110, 170, 210, 160);
pub const SELECT: Rgba = Rgba(230, 220, 160, 255);

pub const EDIT_CURSOR: Rgba = Rgba(230, 200, 120, 200);

pub const FONT: i32 = 10;
pub const FONT_L: i32 = 20;
