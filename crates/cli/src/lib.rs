#![cfg_attr(doc, doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR" ), "/", "README.md")))]
#![cfg_attr(not(doc), doc = "SimplicityHL formatter")]
#![warn(clippy::all, clippy::pedantic, missing_docs, unreachable_pub)]

mod cli;
mod config;
mod error;

pub use cli::{execute, make_opts};
