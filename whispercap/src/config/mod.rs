mod conf;
mod data;

pub use conf::{all, app_name, cache_dir, init, is_first_run, model, preference, save};

#[cfg(feature = "database")]
pub use conf::db_path;
