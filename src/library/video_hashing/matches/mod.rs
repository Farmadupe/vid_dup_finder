mod search_output;

pub use search_output::SearchOutput;

mod match_group;
pub use match_group::MatchGroup;

#[cfg(feature = "gui")]
mod match_group_resolution_thunk;

#[cfg(feature = "gui")]
pub use match_group_resolution_thunk::ResolutionThunk;
