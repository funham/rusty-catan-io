use crate::gameplay::resource::*;

#[derive(Debug, Clone, Copy)]
pub enum HexType {
    Some(Resource),
    Desert,
}

pub struct HexInfo {
    hex_type: HexType,
    number: u8,
}
