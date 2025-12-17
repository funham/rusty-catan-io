use crate::gameplay::resource::*;

#[derive(Debug, Clone, Copy)]
pub enum HexType {
    Some(Resource),
    Desert,
}

#[derive(Debug, Clone, Copy)]
pub struct HexInfo {
    pub hex_type: HexType,
    pub number: u8,
}
