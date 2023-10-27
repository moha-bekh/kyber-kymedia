use log::warn;

use crate::utils;
use crate::{Error, Result};

use avservice_types::{Display, Event};
use libkypc::EventSender;

const LUA_SCRIPT: &str = "avserver_displays.lua";
const ENTRYPOINT: &str = "main";

const LUA_UPDATE_CB: &str = "display_update_cb";
const LUA_DISPLAY_LIST_JSON: &str = "video_display_list_json";

/**
 * Start txproto to enumerate the displays and get the updated.
 *
 * Here is the strategy:
 * 1. Start txproto and run the enumeration at first.
 *    At this point, we don't get the display list
 * 2. Plug a callback to be notified when the display list is updated.
 * 3. Get the final display list.
 */
pub(crate) fn enumerate(
    mut sender: EventSender<Event>,
) -> Result<(txproto_rs::Context, Vec<Display>)> {
    // Run script
    let script = utils::get_resource_path_str(LUA_SCRIPT)?;

    let ctx =
        txproto_rs::Context::new(&script, ENTRYPOINT, None).ok_or(Error::TxprotoCreateFailed)?;

    // Plug update callback
    let cb = move |display: String| {
        let displays = match serde_json::from_str(&display) {
            Ok(displays) => displays,
            Err(err) => {
                warn!("Failed to parse new display list '{display}': {err:?}");
                return;
            }
        };

        let ret = sender.blocking_send(Event::DisplayListUpdated { displays });
        if let Err(err) = ret {
            warn!("Failed to send new display list: {err:?}");
        }
    };

    ctx.set_callback(LUA_UPDATE_CB, cb);

    // Get display list
    let Some(display_list) = ctx.get_global(LUA_DISPLAY_LIST_JSON) else {
        return Err(Error::TxprotoGetGlobalFailed {
            variable: LUA_DISPLAY_LIST_JSON.into(),
        });
    };

    let displays = serde_json::from_str(&display_list).map_err(|e| {
        warn!("Failed to parsed json returned by txproto: {e:?}");
        Error::TxprotoInvalidJson { json: display_list }
    })?;

    Ok((ctx, displays))
}
