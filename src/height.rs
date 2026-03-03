#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Interpolation {
    Nearest,
    #[default]
    Bilinear,
    Bicubic,
}
