use ghost_core::Id128;
use ghost_protocol::{ChatEvent, EventKind, EVENT_VERSION};
fn main() {
    let event = ChatEvent { version: EVENT_VERSION, id: Id128([1;16]), conversation: Id128([2;16]), kind: EventKind::Text { body: "hello over HYDRA/Kaspa".into(), reply_to: None } };
    let encoded = event.encode().expect("event");
    println!("{} bytes before HYDRA encryption", encoded.len());
}
