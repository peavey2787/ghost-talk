use ghost_core::Id128;
use ghost_rooms::{radio, Role};

fn main() {
    let presenter_id = Id128([5; 16]);
    let audience_id = Id128([6; 16]);
    let mut room = radio(Id128([4; 16]), "Kaspa Radio".to_owned());
    room.members.insert(presenter_id, Role::Presenter);
    room.members.insert(audience_id, Role::Audience);
    println!(
        "presenter audio={}, audience audio={}",
        room.can_send_audio(presenter_id),
        room.can_send_audio(audience_id)
    );
}
