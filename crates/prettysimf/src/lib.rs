#![doc(html_root_url = "https://docs.rs/prettysimf")]
#![cfg_attr(doc, doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR" ), "/", "README.md")))]
#![cfg_attr(not(doc), doc = "Prettysimf SDK")]
#![warn(clippy::all, clippy::pedantic, missing_docs, unreachable_pub)]

mod api;
mod config;
mod emitter;
mod error;
mod fmt_processor;
mod newline_style;
mod reporter;
mod simplicity_fmt;
mod source_file;
mod utils;

pub use api::{FormatOptions, PrettySimfError, pretty_simf_please};
pub use config::NewlineStyle;

/// Advanced API shared by command-line tools and formatter integrations.
///
/// Most applications only need [`pretty_simf_please`] and [`FormatOptions`]
/// from the crate root. The driver API additionally exposes configuration,
/// output modes, and stateful formatting sessions.
pub mod driver {
    pub use crate::config::{Color, FmtConfig, PartialConfig};
    pub use crate::fmt_processor::FormatterSession;
    pub use crate::utils::{EmitMode, FormatInput, Verbosity};
}
