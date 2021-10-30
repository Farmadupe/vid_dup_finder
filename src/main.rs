#![allow(clippy::let_and_return)]
#![allow(clippy::len_without_is_empty)]

#[macro_use]
extern crate log;

#[cfg(all(target_family = "unix", feature = "gui"))]
extern crate lazy_static;

mod app;

fn main() {
    let return_code = app::run_app();
    std::process::exit(return_code)
}
