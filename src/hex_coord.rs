use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HexCoord {
    x: i32,
    y: i32,
    z: i32,
}

impl TryFrom<(i32, i32, i32)> for HexCoord {
    type Error = ();
    fn try_from(value: (i32, i32, i32)) -> Result<Self, Self::Error> {
        if value.0 + value.1 + value.2 != 6 {
            return Err(());
        }

        Ok(Self {
            x: value.0,
            y: value.1,
            z: value.2,
        })
    }
}

impl Into<(i32, i32, i32)> for HexCoord {
    fn into(self) -> (i32, i32, i32) {
        (self.x, self.y, self.z)
    }
}

impl HexCoord {
    pub fn e(&self) -> HexCoord {
        HexCoord {
            x: self.x + 1,
            y: self.y,
            z: self.z,
        }
    }

    pub fn w(&self) -> HexCoord {
        HexCoord {
            x: self.x - 1,
            y: self.y,
            z: self.z,
        }
    }

    pub fn ne(&self) -> HexCoord {
        HexCoord {
            x: self.x,
            y: self.y + 1,
            z: self.z - 1,
        }
    }

    pub fn se(&self) -> HexCoord {
        HexCoord {
            x: self.x + 1,
            y: self.y,
            z: self.z - 1,
        }
    }

    pub fn sw(&self) -> HexCoord {
        HexCoord {
            x: self.x - 1,
            y: self.y,
            z: self.z + 1,
        }
    }

    pub fn nw(&self) -> HexCoord {
        HexCoord {
            x: self.x,
            y: self.y - 1,
            z: self.z + 1,
        }
    }
    
    fn distance(&self, other: &HexCoord) -> i32 {
        (self.x - other.x).abs()
            + (self.y - other.y).abs()
            + (self.z - other.z).abs() / 2
    }


    pub fn to_id(&self) -> usize {
        (self.x + self.y * 5 + self.y * 25) as usize
    }

    pub fn to_string(&self) -> String {
        format!("({}, {}, {})", self.x, self.y, self.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let coord = HexCoord { x: 0, y: 0, z: 0 };
        let new_coord = coord.e();
        println!("New coordinate: {}", new_coord.to_string());
    }
}
