#[derive(Debug, Clone, Copy)]
pub enum Resource {
    BRICK,
    WOOD,
    WHEAT,
    SHEEP,
}

pub enum PortType {
    Special(Resource),
    General,
}
