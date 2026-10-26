//! One outgoing chat text on its way to a carrier.

/// Text to send. `retry_wire_id` resends a failed message under its original
/// id; a fresh message gets a new id from the carrier that sends it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OutgoingText {
    pub(crate) body: String,
    pub(crate) retry_wire_id: Option<String>,
}

impl OutgoingText {
    pub(crate) fn new(body: String) -> Self {
        Self {
            body,
            retry_wire_id: None,
        }
    }
}

/// The send pipeline answers `true` when it took ownership of the text; on
/// `false` the caller keeps it (the composer keeps the draft).
pub(crate) type TextSender = yew::Callback<OutgoingText, bool>;
