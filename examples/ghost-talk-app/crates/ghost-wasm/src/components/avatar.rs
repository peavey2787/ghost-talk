use ghost_media::MediaReference;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[derive(Clone, PartialEq, Properties)]
pub struct AvatarProps {
    pub label: String,
    #[prop_or_default]
    pub reference: Option<MediaReference>,
    #[prop_or(48)]
    pub size: u16,
}

#[component(Avatar)]
pub fn avatar(props: &AvatarProps) -> Html {
    let source = use_state(|| None::<String>);
    let failed = use_state(|| false);
    {
        let source = source.clone();
        let failed = failed.clone();
        let reference = props.reference.clone();
        use_effect_with(reference, move |reference| {
            source.set(None);
            failed.set(false);
            if let Some(reference) = reference.clone() {
                spawn_local(async move {
                    match crate::controllers::media::fetch_verified(&reference).await {
                        Ok(media) => source.set(Some(format!(
                            "data:{};base64,{}",
                            media.content_type, media.data_base64
                        ))),
                        Err(_) => failed.set(true),
                    }
                });
            }
            || ()
        });
    }
    let style = format!("width:{}px;height:{}px", props.size, props.size);
    if let Some(source) = source.as_ref() {
        html! { <img class="avatar ghost-avatar" {style} src={source.clone()} alt={props.label.clone()} /> }
    } else {
        let letter = props
            .label
            .chars()
            .next()
            .unwrap_or('G')
            .to_uppercase()
            .to_string();
        let class = classes!("avatar", "ghost-avatar", failed.then_some("avatar-invalid"));
        html! { <div {class} {style} aria-label={props.label.clone()}>{letter}</div> }
    }
}
