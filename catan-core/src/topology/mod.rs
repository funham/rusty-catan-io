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
    fn pos(&self) -> Self::Pos;
}

impl<T: HasPos> HasPos for &T {
    type Pos = <T as HasPos>::Pos;

    fn pos(&self) -> Self::Pos {
        <T as HasPos>::pos(&self)
    }
}
