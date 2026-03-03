#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Interpolation {
    Nearest,
    Bilinear,
    Bicubic,
}

impl Default for Interpolation {
    fn default() -> Self {
        Self::Bilinear
    }
}
