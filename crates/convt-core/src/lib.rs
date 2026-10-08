//! convt-core knows which formats exist, which engines can convert between
//! them, and how to chain engines when no single one covers a conversion.
//! It has no native dependencies; the engines live in `convt-engines`.

mod batch;
mod engine;
mod error;
mod formats;
mod options;
mod publish;
mod registry;

pub use batch::{BatchItem, Event, expand_inputs, run_batch};
pub use engine::{Cancel, Ctx, Engine, Progress, Step};
pub use error::{Error, Result, stderr_tail};
pub use formats::{Category, FORMATS, Format, format_by_extension, format_by_id};
pub use options::{Background, Options, PageRange, Preset, VideoCodec};
pub use registry::{Destination, Job, Output, Plan, Registry};
