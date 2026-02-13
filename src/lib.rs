#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod cli;
pub mod cmake;
pub mod cmake_core;
pub mod cmake_gen;
pub mod command_runner;
pub mod commands;
pub mod conan;
pub mod config;
pub mod dependency;
pub mod testing;
pub mod traits;
pub mod ui;

#[doc(hidden)]
pub mod test_utils;
