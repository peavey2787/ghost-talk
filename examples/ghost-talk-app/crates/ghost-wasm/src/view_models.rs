use crate::model::{Chat, Contact, Room, WalletRecord};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadViewModel {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) last_body: Option<String>,
    pub(crate) unread_count: u32,
    pub(crate) incoming_pending: bool,
    pub(crate) incoming_call: bool,
}

impl ThreadViewModel {
    pub(crate) fn unread_count(&self) -> u32 {
        self.unread_count
    }
}

impl From<&Chat> for ThreadViewModel {
    fn from(chat: &Chat) -> Self {
        Self {
            id: chat.id.clone(),
            label: chat.label.clone(),
            last_body: chat.messages().last().map(|message| message.body.clone()),
            unread_count: chat.unread_count(),
            incoming_pending: chat
                .incoming_request()
                .is_some_and(|request| request.state == "pending"),
            incoming_call: chat
                .incoming_request()
                .is_some_and(|request| request.call_id.is_some()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContactViewModel {
    pub(crate) label: String,
    pub(crate) target: String,
    pub(crate) subtitle: String,
    pub(crate) verified_public: bool,
}

impl From<&Contact> for ContactViewModel {
    fn from(contact: &Contact) -> Self {
        let address = contact.kaspa_address().to_string();
        let target = contact
            .kns_name
            .clone()
            .or_else(|| contact.dotk_name.clone())
            .unwrap_or_else(|| address.clone());
        Self {
            label: contact.label.clone(),
            target: target.clone(),
            subtitle: target,
            verified_public: contact.verified_public,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RoomViewModel {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) participant_count: usize,
}

impl From<&Room> for RoomViewModel {
    fn from(room: &Room) -> Self {
        Self {
            id: room.id.clone(),
            name: room.name.clone(),
            participant_count: room.members().len() + 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WalletViewModel {
    pub(crate) network: String,
    pub(crate) receive_address: String,
    pub(crate) next_receive_index: usize,
}

impl From<&WalletRecord> for WalletViewModel {
    fn from(wallet: &WalletRecord) -> Self {
        Self {
            network: wallet.public.network.clone(),
            receive_address: wallet.public.receive_address().to_string(),
            next_receive_index: wallet.public.next_receive_index,
        }
    }
}
