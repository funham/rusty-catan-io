use catan_remote::{ClientMessage, HostMessage};

pub type ServerToClient = HostMessage;
pub type ClientToServer = ClientMessage;
