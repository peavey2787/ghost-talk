pub(super) fn normalized_override(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub(super) fn is_transport_failure(error: &str) -> bool {
    ghost_kaspa::is_portal_transport_failure(error)
        || error == super::block_pump::BLOCK_STREAM_STALLED
}

#[cfg(test)]
mod tests {
    use super::is_transport_failure;
    use crate::kaspa_gateway::block_pump::BLOCK_STREAM_STALLED;

    #[test]
    fn a_stalled_block_stream_moves_to_the_next_endpoint() {
        assert!(is_transport_failure(BLOCK_STREAM_STALLED));
        assert!(!is_transport_failure("subscription rejected"));
    }
}
