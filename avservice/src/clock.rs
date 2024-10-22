use txproto_rs::Txproto;

use crate::config::ClockType;
use crate::Result;

pub(crate) fn configure_clock(ctx: &Txproto, clock_type: &ClockType) -> Result<()> {
    match clock_type {
        ClockType::ZeroedOrigin => {
            ctx.set_epoch_offset(0)?;
        }
        ClockType::System => {
            ctx.set_epoch_system()?;
        }
    }

    Ok(())
}
