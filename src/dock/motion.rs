//! Time-based Dock motion. No idle timer is needed once a spring settles.
#[derive(Clone, Copy, Default)]
pub(super) struct Spring {
    pub value: f32,
    velocity: f32,
}

impl Spring {
    pub fn rest(value: f32) -> Self { Self { value, velocity: 0.0 } }

    // Closed-form critically damped spring. Reversals preserve velocity, and a
    // delayed Windows message does not turn the integration unstable.
    pub fn advance(&mut self, target: f32, dt: f32, frequency: f32, epsilon: f32) -> bool {
        let offset = self.value - target;
        let slope = self.velocity + frequency * offset;
        let decay = (-frequency * dt).exp();
        self.value = target + (offset + slope * dt) * decay;
        self.velocity = (self.velocity - frequency * slope * dt) * decay;
        if (self.value - target).abs() <= epsilon && self.velocity.abs() <= epsilon * frequency {
            *self = Self::rest(target);
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_time_not_timer_count_controls_the_spring() {
        let mut fine = Spring::default();
        let mut coarse = fine;
        for _ in 0..8 { fine.advance(1.0, 0.01, 22.0, 0.0001); }
        coarse.advance(1.0, 0.08, 22.0, 0.0001);
        assert!((fine.value - coarse.value).abs() < 0.00001);
        assert!((fine.velocity - coarse.velocity).abs() < 0.00001);
    }

    #[test]
    fn rapid_reversal_keeps_position_and_eventually_reaches_exact_rest() {
        let mut spring = Spring::default();
        assert!(spring.advance(1.0, 0.055, 24.0, 0.001));
        let previous = spring.value;
        assert!(spring.advance(0.0, 0.001, 24.0, 0.001));
        assert!((spring.value - previous).abs() < 0.02);
        for _ in 0..100 { spring.advance(0.0, 0.016, 24.0, 0.001); }
        assert_eq!(spring.value, 0.0);
        assert!(!spring.advance(0.0, 0.016, 24.0, 0.001));
    }
}
