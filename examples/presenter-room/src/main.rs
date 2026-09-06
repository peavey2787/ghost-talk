use ghost_core::Id128;
use ghost_rooms::{presenter, Role};

fn main() {
    let audience = Id128([4; 16]);
    let mut room = presenter(Id128([3; 16]), "Town Hall".to_owned());
    room.members.insert(audience, Role::Audience);
    println!(
        "audience can send text: {}",
        room.can_send_text(audience)
    );
}
