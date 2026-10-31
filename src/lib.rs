//! Palette-locked pixel-art sprite generation, sheet slicing and animation export.
pub mod anim;
pub mod atlas;
pub mod dither;
pub mod generate;
pub mod palette;
pub mod sheet;
pub mod slice;

pub use crate::palette::Palette;
