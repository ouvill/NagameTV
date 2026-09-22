pub mod channel_catalog;
pub mod comments;
pub mod program_info;
pub(crate) mod subscriptions;
pub mod subtitles;

mod launch;
pub use launch::{FeatureSet, LaunchPlan};
pub static PLAN: std::sync::OnceLock<LaunchPlan> = std::sync::OnceLock::new();
