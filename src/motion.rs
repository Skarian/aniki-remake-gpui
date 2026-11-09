use std::time::{Duration, Instant};

/// Short, interruptible motion. Retargeting samples the current position rather
/// than restarting at the previous destination, so held input stays continuous.
#[derive(Clone)]
pub struct Tween {
    from: f32,
    pub target: f32,
    started: Instant,
    duration: Duration,
}
impl Tween {
    pub fn new(value: f32, now: Instant) -> Self {
        Self {
            from: value,
            target: value,
            started: now,
            duration: Duration::ZERO,
        }
    }
    pub fn value(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.target;
        }
        let t = (now.saturating_duration_since(self.started).as_secs_f32()
            / self.duration.as_secs_f32())
        .clamp(0., 1.);
        let eased = 1. - (1. - t).powi(3);
        self.from + (self.target - self.from) * eased
    }
    pub fn to(&mut self, target: f32, millis: u64, now: Instant) {
        if self.target == target {
            return;
        }
        self.from = self.value(now);
        self.target = target;
        self.started = now;
        self.duration = Duration::from_millis(millis);
    }
    /// Page opacity starts and ends gently, unlike the quick response of rails.
    pub fn smooth_value(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.target;
        }
        let t = (now.saturating_duration_since(self.started).as_secs_f32()
            / self.duration.as_secs_f32())
        .clamp(0., 1.);
        self.from + (self.target - self.from) * t * t * (3. - 2. * t)
    }
    pub fn active(&self, now: Instant) -> bool {
        self.from != self.target && now.saturating_duration_since(self.started) < self.duration
    }
}

pub struct Rail {
    pub offset: Tween,
    pub selection: Vec<Tween>,
    first: usize,
    initialized: bool,
}
impl Rail {
    pub fn new(now: Instant) -> Self {
        Self {
            offset: Tween::new(0., now),
            selection: Vec::new(),
            first: 0,
            initialized: false,
        }
    }
    pub fn reset(&mut self) {
        self.initialized = false;
    }
    pub fn sync(&mut self, count: usize, selected: usize, slots: usize, now: Instant) {
        let slots = slots.max(1);
        if !self.initialized || self.selection.len() != count {
            self.first = selected
                .saturating_sub(slots - 1)
                .min(count.saturating_sub(slots));
            self.offset = Tween::new(self.first as f32, now);
            self.selection = (0..count)
                .map(|i| Tween::new(if i == selected { 1. } else { 0. }, now))
                .collect();
            self.initialized = true;
        }
        // Keep the viewport anchored until selection actually crosses its edge.
        if selected < self.first {
            self.first = selected;
        }
        if selected >= self.first + slots {
            self.first = selected + 1 - slots;
        }
        self.first = self.first.min(count.saturating_sub(slots));
        self.offset.to(self.first as f32, 240, now);
        for (i, scale) in self.selection.iter_mut().enumerate() {
            scale.to(if i == selected { 1. } else { 0. }, 160, now);
        }
    }
    pub fn active(&self, now: Instant) -> bool {
        self.offset.active(now) || self.selection.iter().any(|s| s.active(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rail_waits_until_selection_crosses_visible_edge_in_both_directions() {
        let now = Instant::now();
        let mut rail = Rail::new(now);
        for selected in 0..8 {
            rail.sync(14, selected, 8, now);
            assert_eq!(rail.offset.target, 0.);
        }
        rail.sync(14, 8, 8, now);
        assert_eq!(rail.offset.target, 1.);
        rail.sync(14, 1, 8, now);
        assert_eq!(rail.offset.target, 1.);
        rail.sync(14, 0, 8, now);
        assert_eq!(rail.offset.target, 0.);
    }
    #[test]
    fn retargeting_during_animation_preserves_current_position_and_settles() {
        let now = Instant::now();
        let mut t = Tween::new(0., now);
        t.to(1., 240, now);
        let halfway = now + Duration::from_millis(100);
        let before = t.value(halfway);
        assert!(before > 0. && before < 1.);
        t.to(2., 240, halfway);
        assert_eq!(before, t.value(halfway));
        assert_eq!(t.value(halfway + Duration::from_millis(240)), 2.);
        assert!(!t.active(halfway + Duration::from_millis(240)));
    }
    #[test]
    fn rail_handles_wrap_filter_changes_and_resize() {
        let now = Instant::now();
        let mut rail = Rail::new(now);
        rail.sync(14, 13, 8, now);
        assert_eq!(rail.offset.target, 6.);
        rail.sync(14, 0, 8, now);
        assert_eq!(rail.offset.target, 0.);
        rail.reset();
        rail.sync(5, 4, 8, now);
        assert_eq!(rail.offset.target, 0.);
        rail.sync(5, 4, 3, now);
        assert_eq!(rail.offset.target, 2.);
        rail.sync(0, 0, 3, now);
        assert_eq!(rail.offset.target, 0.);
    }
}
