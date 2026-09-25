/// Per-envelope processing outcome. One bad packet must never abort unrelated
/// packets later in the mailbox.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PacketDisposition {
    Consumed,
    Deferred,
    RejectedPermanent,
    Retryable,
}

impl PacketDisposition {
    pub fn removes_envelope(self) -> bool {
        matches!(self, Self::Consumed | Self::RejectedPermanent)
    }
}
