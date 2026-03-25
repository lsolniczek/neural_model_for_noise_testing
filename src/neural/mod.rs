pub mod fhn;
pub mod jansen_rit;
mod tests;

pub use fhn::{FhnModel, FhnResult};
pub use jansen_rit::{BandPowers, BilateralResult, JansenRitModel, JansenRitResult, simulate_bilateral, simulate_tonotopic};
