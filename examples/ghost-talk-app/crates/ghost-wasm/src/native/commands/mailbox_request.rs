use serde_json::{json, Value};

use super::Profile;

pub(super) struct MailboxRequestContext<'a> {
    profile: &'a Profile,
    password: &'a str,
    destination: &'a str,
    identity_id: &'a str,
}

impl<'a> MailboxRequestContext<'a> {
    pub(super) fn from_profile(
        profile: &'a Profile,
        password: &'a str,
        destination: &'a str,
    ) -> Result<Self, String> {
        profile
            .wallet
            .as_ref()
            .ok_or_else(|| "No wallet configured".to_string())?;
        let identity_id = profile
            .hydra_identity_id
            .as_deref()
            .ok_or_else(|| "HYDRA identity is unavailable".to_string())?;
        Ok(Self {
            profile,
            password,
            destination,
            identity_id,
        })
    }

    pub(super) fn contact_request(
        &self,
        request_id: &str,
        room: Option<(&str, &str)>,
        call_id: Option<&str>,
    ) -> Value {
        let wallet = self
            .profile
            .wallet
            .as_ref()
            .expect("context requires wallet");
        let (room_id, room_name) = room
            .map(|(id, name)| (json!(id), json!(name)))
            .unwrap_or((Value::Null, Value::Null));
        json!({
            "profileId": self.profile.id,
            "password": self.password,
            "identityId": self.identity_id,
            "senderDisplayName": self.profile.label,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "destination": self.destination,
            "requestId": request_id,
            "roomId": room_id,
            "roomName": room_name,
            "callId": call_id.map(Value::from).unwrap_or(Value::Null),
            "callAction": call_id.map(|_| Value::from("request")).unwrap_or(Value::Null),
            "feeSompi": "0",
            "wrpcEndpoint": wallet.wrpc_endpoint,
        })
    }
}
