//! Browser mailbox commands: contact requests, KKTP mailbox send/receive,
//! control carriers, and realtime carriers over Kaspa.

mod common;
mod control;
mod receive;
mod request;
mod send;

use serde_json::Value;

use crate::native::browser_host::support::util::to_value;

pub(in crate::native::browser_host) use common::{discard, frame};
pub(in crate::native::browser_host) use receive::open_first;
use request::send_contact_request;
pub(in crate::native::browser_host) use send::first_message;

pub(in crate::native::browser_host) async fn invoke(
    command: &str,
    args: &Value,
) -> Result<Value, String> {
    match command {
        "mailbox_send_contact_request" => to_value(send_contact_request(args).await?),
        "hydra_receive_mailbox" => receive::receive(args),
        _ => to_value(send::invoke(command, args).await?),
    }
}
