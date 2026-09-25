use super::view::{spawn_local, Chat, ChatProps, ChatUiState};
use yew::prelude::*;

pub(crate) fn delete_request_callback(props: &ChatProps, chat: Chat) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    let on_select = props.on_select.clone();
    Callback::from(move |_| {
        on_update.emit(crate::controllers::chat::delete_patch(&profile, &chat));
        on_select.emit(String::new());
    })
}

pub(crate) fn leave_chat_callback(
    props: &ChatProps,
    state: &ChatUiState,
    chat: Chat,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let on_update = props.on_update.clone();
    let on_select = props.on_select.clone();
    let on_wallet_public = props.on_wallet_public.clone();
    let status = state.status.clone();
    Callback::from(move |_| {
        let plan = match crate::controllers::chat::prepare_leave(&profile, &chat) {
            Ok(value) => value,
            Err(error) => {
                status.set(error);
                return;
            }
        };
        on_update.emit(plan.patch.clone());
        on_select.emit(String::new());
        status.set("Chat left.".into());
        spawn_leave_notification(
            plan,
            chat.clone(),
            password.clone(),
            on_wallet_public.clone(),
            status.clone(),
        );
    })
}

fn spawn_leave_notification(
    plan: crate::controllers::chat::LeaveChatPlan,
    chat: Chat,
    password: String,
    on_wallet_public: Callback<crate::model::WalletProjection>,
    status: UseStateHandle<String>,
) {
    let Some(peer) = plan.peer.clone() else {
        return;
    };
    if plan.keep_transport {
        return;
    }
    spawn_local(async move {
        match crate::controllers::chat::notify_leave(&plan.local_profile, &chat, &peer, &password)
            .await
        {
            Ok(Some(wallet)) => on_wallet_public.emit(wallet),
            Ok(None) => {}
            Err(error) => status.set(error),
        }
    });
}

pub(crate) fn rejoin_chat_callback(
    props: &ChatProps,
    state: &ChatUiState,
    chat: Chat,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    let status = state.status.clone();
    Callback::from(move |_| {
        let profile = profile.clone();
        let chat = chat.clone();
        let on_update = on_update.clone();
        let status = status.clone();
        spawn_local(async move {
            match crate::controllers::chat::rejoin(&profile, &chat).await {
                Ok((patch, message)) => {
                    on_update.emit(patch);
                    status.set(message.into());
                }
                Err(error) => status.set(error),
            }
        });
    })
}
