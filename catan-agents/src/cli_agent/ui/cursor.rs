use std::ops::{Add, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CursorPosition {
    pub x: i32,
    pub y: i32,
}

impl CursorPosition {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn transposed(&self) -> Self {
        Self {
            x: self.y,
            y: self.x,
        }
    }
}

impl Add for CursorPosition {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::Output {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub for CursorPosition {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Mul<i32> for CursorPosition {
    type Output = CursorPosition;

    fn mul(self, rhs: i32) -> Self::Output {
        Self::Output {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Into<CursorPosition> for (i32, i32) {
    fn into(self) -> CursorPosition {
        CursorPosition {
            x: self.0,
            y: self.1,
        }
    }
}
