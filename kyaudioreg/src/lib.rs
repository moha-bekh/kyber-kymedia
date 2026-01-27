mod average;
mod buffer;
mod compensator;
mod regulator;

#[cfg(feature = "capi")]
mod capi;

pub mod memory;

pub use regulator::{AudioRegulatorPull, AudioRegulatorPush};

#[allow(dead_code)]
pub struct NotAvailable;
