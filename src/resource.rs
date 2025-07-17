#[derive(Clone, Copy)]
pub enum Resource {
    BRICK,
    WOOD,
    WHEAT,
    SHEEP,
}

pub struct Port {
    res: Option<Resource>,
}
