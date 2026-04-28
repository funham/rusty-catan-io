use catan_agents::remote_agent::{CliToHost, HostToCli};

pub type ServerToClient = HostToCli;
pub type ClientToServer = CliToHost;
