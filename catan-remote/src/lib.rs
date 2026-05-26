pub mod config;
pub mod frame;
pub mod protocol;
pub mod runtime_adapter;
pub mod transport;
pub mod tui_adapter;
pub mod tui_logging;

pub use frame::{NonblockingFrameReader, read_frame, write_frame};
pub use protocol::{
    ClientMessage, DecisionRequestEnvelope, DecisionRequestFrame, DecisionResponseFrame,
    HostMessage, LegalBuildOptions, LegalDecisionOptions, RemoteLogLevel, RemoteRole,
};
