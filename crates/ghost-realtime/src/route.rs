use serde::{Deserialize, Serialize};

/// User-facing realtime route preference. Direct transport details (relay,
/// WebRTC, WebSocket, QUIC) stay inside p2p-net and are never route choices.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum RoutePreference {
    /// p2p-net when available, Kaspa on unavailability or send failure.
    Automatic,
    /// Kaspa only; no direct transport is attempted.
    KaspaOnly,
}

/// A physical carrier for sealed realtime bytes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum RealtimeCarrier {
    P2pNet,
    Kaspa,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteDecision {
    pub primary: RealtimeCarrier,
    pub fallback: Option<RealtimeCarrier>,
}

/// Carriers to try, in order, for a preference.
pub fn route_order(preference: RoutePreference) -> &'static [RealtimeCarrier] {
    match preference {
        RoutePreference::Automatic => &[RealtimeCarrier::P2pNet, RealtimeCarrier::Kaspa],
        RoutePreference::KaspaOnly => &[RealtimeCarrier::Kaspa],
    }
}

/// Resolve a concrete send decision given current p2p-net availability.
pub fn route(preference: RoutePreference, p2p_available: bool) -> RouteDecision {
    let mut usable = route_order(preference)
        .iter()
        .copied()
        .filter(|carrier| p2p_available || *carrier != RealtimeCarrier::P2pNet);
    let primary = usable.next().unwrap_or(RealtimeCarrier::Kaspa);
    RouteDecision {
        primary,
        fallback: usable.next(),
    }
}

/// Carrier preference for chat text. Kaspa is durable (anchored on chain);
/// p2p-net text is ephemeral and never stored on Kaspa.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum TextPreference {
    /// Every message is anchored on Kaspa (default).
    #[default]
    Kaspa,
    /// p2p-net while connected (not stored on Kaspa), otherwise Kaspa.
    P2pPreferred,
    /// p2p-net only; text is never stored on Kaspa and fails without a route.
    P2pOnly,
}

impl TextPreference {
    /// Parse the persisted setting label; unknown values stay durable.
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "p2p preferred" => Self::P2pPreferred,
            "p2p only" => Self::P2pOnly,
            _ => Self::Kaspa,
        }
    }
}

/// Carrier for one chat text, or `None` when the preference forbids every
/// currently usable carrier (p2p-only without a connected route).
pub fn text_route(preference: TextPreference, p2p_connected: bool) -> Option<RouteDecision> {
    let direct = RouteDecision {
        primary: RealtimeCarrier::P2pNet,
        fallback: None,
    };
    let durable = RouteDecision {
        primary: RealtimeCarrier::Kaspa,
        fallback: None,
    };
    match (preference, p2p_connected) {
        (TextPreference::P2pPreferred, true) => Some(RouteDecision {
            fallback: Some(RealtimeCarrier::Kaspa),
            ..direct
        }),
        (TextPreference::P2pOnly, true) => Some(direct),
        (TextPreference::P2pOnly, false) => None,
        _ => Some(durable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_prefers_p2p_net_then_kaspa() {
        assert_eq!(
            route_order(RoutePreference::Automatic),
            &[RealtimeCarrier::P2pNet, RealtimeCarrier::Kaspa]
        );
        assert_eq!(
            route(RoutePreference::Automatic, true),
            RouteDecision {
                primary: RealtimeCarrier::P2pNet,
                fallback: Some(RealtimeCarrier::Kaspa)
            }
        );
    }

    #[test]
    fn unavailable_p2p_and_kaspa_only_never_attempt_direct_transport() {
        let expected = RouteDecision {
            primary: RealtimeCarrier::Kaspa,
            fallback: None,
        };
        assert_eq!(route(RoutePreference::Automatic, false), expected);
        assert_eq!(route(RoutePreference::KaspaOnly, true), expected);
        assert_eq!(
            route_order(RoutePreference::KaspaOnly),
            &[RealtimeCarrier::Kaspa]
        );
    }

    #[test]
    fn route_values_serialize_without_transport_mechanics() {
        let json =
            serde_json::to_string(&(RoutePreference::Automatic, RealtimeCarrier::P2pNet)).unwrap();
        let lower = json.to_ascii_lowercase();
        assert!(!lower.contains("webrtc") && !lower.contains("quic"));
    }

    #[test]
    fn text_defaults_to_durable_kaspa() {
        assert_eq!(
            TextPreference::from_setting("anything"),
            TextPreference::Kaspa
        );
        assert_eq!(TextPreference::default(), TextPreference::Kaspa);
        for connected in [true, false] {
            let decision = text_route(TextPreference::Kaspa, connected).unwrap();
            assert_eq!(decision.primary, RealtimeCarrier::Kaspa);
            assert_eq!(decision.fallback, None);
        }
    }

    #[test]
    fn p2p_text_preferences_never_silently_anchor_p2p_only_text() {
        let preferred = TextPreference::from_setting("P2P preferred");
        let only = TextPreference::from_setting(" p2p ONLY ");
        assert_eq!(preferred, TextPreference::P2pPreferred);
        assert_eq!(only, TextPreference::P2pOnly);
        let decision = text_route(preferred, true).unwrap();
        assert_eq!(decision.primary, RealtimeCarrier::P2pNet);
        assert_eq!(decision.fallback, Some(RealtimeCarrier::Kaspa));
        assert_eq!(
            text_route(preferred, false).unwrap().primary,
            RealtimeCarrier::Kaspa
        );
        assert_eq!(text_route(only, true).unwrap().fallback, None);
        assert_eq!(text_route(only, false), None);
    }
}
