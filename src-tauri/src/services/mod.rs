//! Backend services. Each submodule owns one domain and exposes plain data
//! types; the `commands` layer is a thin adapter that maps them onto Tauri
//! commands and does no work of its own.

pub mod ai;
pub mod apps;
pub mod battery;
pub mod cleanup;
pub mod config;
pub mod db;
pub mod health;
pub mod process;
pub mod security;
pub mod startup;
pub mod storage;
pub mod system;
pub mod wmi_thread;
