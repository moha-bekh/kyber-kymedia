use crate::{Error, Result};

use avservice_types::{Display, Event};
use libkypc::EventSender;

use crate::Context;

pub(crate) fn enumerate(mut sender: EventSender<Event>) -> Result<(Context, Vec<Display>)> {
    Err(Error::Unimplemented)
}
