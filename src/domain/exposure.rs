#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Exposure(f64);

impl Exposure {
    pub const MIN: f64 = -1.0;
    pub const MAX: f64 = 1.0;

    pub fn new(value: f64) -> Option<Self> {
        if (Self::MIN..=Self::MAX).contains(&value) {
            Some(Self(value))
        } else {
            None
        }
    }

    pub fn value(self) -> f64 {
        self.0
    }
}
