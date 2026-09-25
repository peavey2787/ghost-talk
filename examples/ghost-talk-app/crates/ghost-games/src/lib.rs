#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct PongState {
    pub tick: u64,
    pub ball_x: f32,
    pub ball_y: f32,
    pub vx: f32,
    pub vy: f32,
    pub left_y: f32,
    pub right_y: f32,
    pub left_score: u8,
    pub right_score: u8,
}

impl Default for PongState {
    fn default() -> Self {
        Self {
            tick: 0,
            ball_x: 0.5,
            ball_y: 0.5,
            vx: 0.006,
            vy: 0.004,
            left_y: 0.5,
            right_y: 0.5,
            left_score: 0,
            right_score: 0,
        }
    }
}

impl PongState {
    pub fn step(&mut self, left: f32, right: f32) {
        self.tick += 1;
        self.left_y = (self.left_y + left.clamp(-1.0, 1.0) * 0.02).clamp(0.08, 0.92);
        self.right_y = (self.right_y + right.clamp(-1.0, 1.0) * 0.02).clamp(0.08, 0.92);
        self.ball_x += self.vx;
        self.ball_y += self.vy;

        if self.ball_y <= 0.0 || self.ball_y >= 1.0 {
            self.vy = -self.vy;
        }

        if self.ball_x < 0.0 {
            self.right_score = self.right_score.saturating_add(1);
            self.reset(1.0);
        } else if self.ball_x > 1.0 {
            self.left_score = self.left_score.saturating_add(1);
            self.reset(-1.0);
        }
    }

    fn reset(&mut self, dir: f32) {
        self.ball_x = 0.5;
        self.ball_y = 0.5;
        self.vx = 0.006 * dir;
        self.vy = 0.004;
    }

    pub fn state_hash(&self) -> [u8; 32] {
        *blake3::hash(&serde_json::to_vec(self).unwrap_or_default()).as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pong_is_deterministic() {
        let mut a = PongState::default();
        let mut b = PongState::default();

        for _ in 0..100 {
            a.step(0.2, -0.3);
            b.step(0.2, -0.3);
        }

        assert_eq!(a, b);
        assert_eq!(a.state_hash(), b.state_hash());
    }
}
