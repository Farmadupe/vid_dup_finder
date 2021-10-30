#![allow(clippy::let_and_return)]
#![allow(clippy::len_without_is_empty)]

#[macro_use]
extern crate log;

#[cfg(all(target_family = "unix", feature = "gui"))]
extern crate lazy_static;

mod app_cfg;
mod app_fns;
mod arg_parse;
mod errors;
#[cfg(all(target_family = "unix", feature = "gui"))]
mod gui;
#[cfg(all(target_family = "unix", feature = "gui"))]
mod resolution_thunk;
mod search_output;

pub(crate) use app_cfg::*;
pub(crate) use errors::*;
pub(crate) use search_output::SearchOutput;
#[cfg(all(target_family = "unix", feature = "gui"))]
pub(crate) use gui::run_gui;
#[cfg(all(target_family = "unix", feature = "gui"))]
pub(crate) use resolution_thunk::*;

pub use app_fns::run_app;
pub use arg_parse::generate_shell_completions;
