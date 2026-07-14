//! Streaming novelty model: colour intensity as a function of statistical
//! novelty rather than pattern class.
//!
//! The engine masks the variable-value spans of a line (numbers, IPs, PIDs,
//! sizes, versions, dates, times, addresses) to produce a *template*, hashes it,
//! and feeds the hash here. [`NoveltyModel`] keeps an online frequency table of
//! templates seen so far. A template's *novelty score* falls as it recurs:
//! first-seen and rare templates score near `1.0` (render bright/bold); templates
//! that dominate a repetitive stream score near `0.0` (render dim). No rule and
//! no prior knowledge of the log format is required — a `tail -f` of repetitive
//! lines self-quiets while an anomalous template lights up.
//!
//! Optional exponential decay ages the counts so a pattern that stops recurring
//! is eventually forgotten and treated as novel again if it returns.

use std::collections::HashMap;

/// How many observations between decay sweeps. A sweep is `O(distinct templates)`,
/// so batching keeps the streaming cost near zero on hot loops.
const DECAY_INTERVAL: u64 = 512;

/// An online frequency model over line templates.
///
/// `seen[hash]` is how many times that template has been observed (post-decay,
/// when decay is enabled); `total` is the running count of observations.
#[derive(Debug, Clone, Default)]
pub struct NoveltyModel {
    seen: HashMap<u64, u32>,
    total: u64,
    /// Multiplicative aging factor in `(0, 1]` applied to every count each
    /// [`DECAY_INTERVAL`] observations. `None` = never forget (pure cumulative
    /// frequency). `1.0` is equivalent to `None`.
    decay: Option<f32>,
}

impl NoveltyModel {
    /// A model with no decay: counts accumulate forever.
    pub fn new() -> NoveltyModel {
        NoveltyModel::default()
    }

    /// A model that ages its counts by `factor` every `DECAY_INTERVAL`
    /// observations. `factor` is clamped to `(0, 1]`; a value `>= 1.0` disables
    /// decay (equivalent to [`NoveltyModel::new`]).
    pub fn with_decay(factor: f32) -> NoveltyModel {
        let factor = factor.clamp(f32::MIN_POSITIVE, 1.0);
        NoveltyModel {
            seen: HashMap::new(),
            total: 0,
            decay: if factor >= 1.0 { None } else { Some(factor) },
        }
    }

    /// Record one observation of `hash` and return its novelty score in `[0, 1]`.
    ///
    /// A first-seen template scores `1.0`. Thereafter the score is `1 / (k + 1)`
    /// where `k` is the number of prior sightings, so it decays monotonically
    /// toward `0` as the template recurs: `0.5`, `0.33`, `0.25`, …
    pub fn observe_and_score(&mut self, hash: u64) -> f32 {
        self.total = self.total.wrapping_add(1);
        let count = self.seen.entry(hash).or_insert(0);
        let prior = *count;
        *count = count.saturating_add(1);
        self.maybe_decay();
        if prior == 0 {
            1.0
        } else {
            1.0 / (prior as f32 + 1.0)
        }
    }

    /// Number of distinct templates currently tracked.
    pub fn distinct(&self) -> usize {
        self.seen.len()
    }

    /// Total observations recorded.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Age every count by the decay factor once the interval elapses, dropping
    /// templates whose count rounds to zero (they become "first-seen" again).
    fn maybe_decay(&mut self) {
        let Some(factor) = self.decay else { return };
        if !self.total.is_multiple_of(DECAY_INTERVAL) {
            return;
        }
        self.seen.retain(|_, v| {
            *v = (*v as f32 * factor) as u32;
            *v > 0
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_seen_scores_high_repeats_decay_toward_dim() {
        let mut m = NoveltyModel::new();
        // First sighting of a template blazes at full novelty.
        let h = 0xdead_beef;
        assert_eq!(m.observe_and_score(h), 1.0, "first-seen is maximally novel");

        // Repeated sightings decay monotonically toward zero.
        let mut prev = 1.0;
        let mut last = 1.0;
        for _ in 0..12 {
            let s = m.observe_and_score(h);
            assert!(
                s < prev,
                "score must strictly decrease on repeat: {s} !< {prev}"
            );
            prev = s;
            last = s;
        }
        // After a dozen repeats the template is deep in "dim" territory.
        assert!(
            last < 0.15,
            "repeated template decays toward dim, got {last}"
        );
    }

    #[test]
    fn distinct_templates_tracked_independently() {
        let mut m = NoveltyModel::new();
        let a = 0x1111;
        let b = 0x2222;
        // Hammer template `a` so it becomes noise.
        for _ in 0..20 {
            m.observe_and_score(a);
        }
        // A brand-new template `b` is still maximally novel — `a`'s frequency
        // does not bleed into `b`'s score.
        assert_eq!(
            m.observe_and_score(b),
            1.0,
            "an unrelated new template is unaffected by another's frequency"
        );
        assert_eq!(m.distinct(), 2, "two templates tracked separately");
    }

    #[test]
    fn decay_forgets_a_silenced_pattern() {
        // With aggressive decay, a template hammered then left silent while other
        // traffic flows is eventually forgotten and scores as novel again.
        let mut m = NoveltyModel::with_decay(0.5);
        let noisy = 0xabcd;
        for _ in 0..30 {
            m.observe_and_score(noisy);
        }
        // Flow unrelated traffic to cross decay-interval boundaries and age
        // `noisy` out of the table (30 halved to <1 needs five sweeps).
        for i in 0..(DECAY_INTERVAL * 6) {
            m.observe_and_score(0x5000_0000 + i);
        }
        // `noisy` has been decayed away; its next sighting reads as first-seen.
        assert_eq!(
            m.observe_and_score(noisy),
            1.0,
            "a silenced pattern is forgotten and re-lights as novel"
        );
    }
}
