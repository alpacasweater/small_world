/// Sampling mode for interpolating gridded geoid/terrain data between posts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Interpolation {
    /// Snap to the closest grid post; fastest, piecewise-constant output.
    Nearest,
    /// Weighted average of the surrounding 2x2 grid posts; continuous output.
    #[default]
    Bilinear,
    /// Catmull-Rom cubic over the surrounding 4x4 grid posts; smoothest output.
    Bicubic,
}
