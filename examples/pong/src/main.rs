use ghost_games::PongState;

fn main() {
    let mut game = PongState::default();
    for _ in 0..120 {
        game.step(0.0, 0.0);
    }
    println!(
        "score {}:{} hash {:?}",
        game.left_score,
        game.right_score,
        game.state_hash()
    );
}
