pub mod action {
    pub use super::command::{
        ChooseRobbedPlayerCommand as ChoosePlayerToRobAction, DropHalfCommand as DropHalfAction,
        InitCommand as InitAction, InitialPlacementCommand as InitStageAction,
        MoveRobberCommand as MoveRobbersAction, PostDevCardCommand as PostDevCardAction,
        PostDiceCommand as PostDiceAction, RegularCommand as RegularAction,
    };
}
pub mod command;
pub mod decider;
pub mod decision;
pub mod engine;
pub mod event;
pub mod index;
pub mod init;
pub mod input;
pub mod legal;
pub mod lifecycle;
pub mod output;
pub mod phase;
pub mod projector;
pub mod query;
pub mod reducer;
pub mod run;
pub mod state;
pub mod trade;
pub mod view;
