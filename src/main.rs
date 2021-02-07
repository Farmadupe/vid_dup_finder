#![feature(str_split_once)]
#![feature(const_fn_floating_point_arithmetic)]
#![allow(clippy::let_and_return)]
#![allow(dead_code)]

#[macro_use]
extern crate serde_big_array;

#[macro_use]
extern crate log;

mod app;
mod generic_filesystem_cache;
mod library;
#[cfg(test)]
mod test;

fn main() {
    crate::app::run_app()
}
