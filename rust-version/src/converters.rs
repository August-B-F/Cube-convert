pub mod shared;
pub mod wind;
pub mod bpm;
pub mod clouds;
pub mod rgb;
pub mod text;

pub use wind::convert_wind;
pub use bpm::convert_bpm;
pub use clouds::convert_clouds;
pub use rgb::convert_rgb;
pub use text::convert_text;

use crossbeam_channel::Sender;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum Progress {
    Init { total: usize },
    Start { name: String },
    Update { name: String, fraction: f32 },
    Done { name: String },
    Error { name: String, error: String },
}

pub type ProgressTx = Sender<Progress>;
pub type CancelFlag = Arc<AtomicBool>;
