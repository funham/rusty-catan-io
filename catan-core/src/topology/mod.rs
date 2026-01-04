pub mod collision;
pub mod graph;
pub mod hex;
pub mod intersection;
pub mod path;

pub use hex::*;
pub use intersection::*;
pub use path::*;

pub trait HasPos {
    type Pos;
    fn get_pos(&self) -> Self::Pos;
}

impl<T: HasPos> HasPos for &T {
    type Pos = <T as HasPos>::Pos;

    fn get_pos(&self) -> Self::Pos {
        <T as HasPos>::get_pos(&self)
    }
}
