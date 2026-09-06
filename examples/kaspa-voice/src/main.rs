use ghost_core::Id128;
use ghost_voice::{MediaFrame,MediaKey};
fn main() { let key=MediaKey::random(); let frame=MediaFrame::seal(&key,None,Id128([5;16]),0,b"opus-frame").expect("seal"); assert_eq!(frame.open(&key).unwrap(),b"opus-frame"); println!("authenticated GTVA media frame ready"); }
