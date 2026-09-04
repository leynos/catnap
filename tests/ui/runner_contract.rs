use std::{error::Error, time::Duration};

use catnap::{RunConfig, StdMonotonicClock, ThreadLogicalSleeper, run_sleep};

fn main() -> Result<(), Box<dyn Error>> {
    let clock = StdMonotonicClock;
    let mut sleeper = ThreadLogicalSleeper::new(Duration::from_secs(1))?;
    let mut output = Vec::new();
    let config = RunConfig::new(Duration::ZERO, "en-GB");

    run_sleep(&clock, &mut sleeper, &mut output, &config)?;
    Ok(())
}
