use crate::{Chat, ChatStore, Message};

pub(crate) fn chat_by_id_mut<'a>(chats: &'a mut ChatStore, id: &str) -> Option<&'a mut Chat> {
    chats.as_mut_slice().iter_mut().find(|chat| chat.id == id)
}

pub(crate) fn message_by_index_mut(
    chats: &mut ChatStore,
    chat_index: usize,
    message_index: usize,
) -> Option<&mut Message> {
    chats
        .as_mut_slice()
        .get_mut(chat_index)?
        .messages
        .get_mut(message_index)
}

pub(crate) fn message_by_id_mut<'a>(
    chats: &'a mut ChatStore,
    chat_id: &str,
    message_id: &str,
) -> Option<&'a mut Message> {
    chat_by_id_mut(chats, chat_id)?
        .messages
        .iter_mut()
        .find(|message| message.id == message_id)
}
