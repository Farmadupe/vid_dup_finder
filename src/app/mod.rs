mod app_cfg;
mod app_fns;
mod arg_parse;
mod errors;

#[cfg(feature = "gui")]
mod gui;

//exports
pub use app_cfg::{AppCfg, OutputCfg};
pub use app_fns::{sort_thunks, *};
pub use arg_parse::parse_args;
pub use errors::AppError;
#[cfg(feature = "gui")]
pub use gui::run_gui::run_gui;
