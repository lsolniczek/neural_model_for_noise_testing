pub mod fhn;
pub mod jansen_rit;

pub use fhn::{FhnModel, FhnResult};
pub use jansen_rit::{BandPowers, JansenRitModel, JansenRitResult, simulate_tonotopic};
