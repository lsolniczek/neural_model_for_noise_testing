pub mod fhn;
pub mod jansen_rit;
pub mod performance;
mod tests;
pub mod wilson_cowan;

pub use fhn::{FhnModel, FhnResult};
pub use jansen_rit::{
    simulate_bilateral, simulate_tonotopic, BandPowers, BilateralResult, FastInhibParams,
    JansenRitModel, JansenRitResult,
};
pub use performance::PerformanceVector;
pub use wilson_cowan::{WilsonCowanModel, WilsonCowanResult};
