#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Resource {
    Brick,
    Wood,
    Wheat,
    Sheep,
}

pub enum PortType {
    Special(Resource),
    General,
}

pub struct ResourceStorage {
    data: [usize; 4]
}

impl ResourceStorage {
    
}
