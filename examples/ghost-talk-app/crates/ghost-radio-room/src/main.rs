use ghost_rooms::{RoomMode, RoomPolicies};

fn main() {
    let policies = RoomPolicies::for_mode(RoomMode::Radio);
    println!("{:?}", policies);
}
