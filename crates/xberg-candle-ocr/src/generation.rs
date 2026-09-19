//! Shared repeat-guard for the greedy decode loops in the candle VLM OCR engines.
//!
//! A page with a dense, visually-repetitive region (a signature block, a ruled table with wide
//! identical cells) can push a plain argmax decoder into a run of tokens that reproduces the
//! same short unit indefinitely instead of terminating on EOS. This module detects an exactly
//! periodic tail and lets the caller stop early instead of burning the rest of the token budget
//! on invented output.

/// Number of trailing token ids examined for an exactly periodic run.
///
// ~keep: a wide table row of identical cells tokenises with period 2-3, so 32 tokens (~11-16
// identical cells) is well within legitimate output that real financial tables hit. 64 tokens
// (~21+ repeats) is outside anything a real page produces and costs at most 64 wasted decode
// steps to detect. The reference DeepSeek-OCR implementation instead bans repeated n-grams via
// `no_repeat_ngram_size=20`, a logit-level ban that needs per-step top-k tracking; this
// stop-on-detect guard works with pure argmax decoding and no extra per-step cost.
pub const REPEAT_GUARD_WINDOW: usize = 64;

/// Largest period, in tokens, considered a degenerate repeat.
pub const REPEAT_GUARD_MAX_PERIOD: usize = 8;

/// Return `Some(period)` when the last `window` ids of `ids` are exactly periodic with some
/// period in `1..=max_period`, i.e. `ids[i] == ids[i - period]` for every index in the window.
/// Returns `None` when `ids` is shorter than `window` or no such period exists.
#[must_use]
pub fn degenerate_tail_period(ids: &[u32], window: usize, max_period: usize) -> Option<usize> {
    if ids.len() < window || window == 0 {
        return None;
    }
    let tail = &ids[ids.len() - window..];
    (1..=max_period.min(window)).find(|&period| is_periodic(tail, period))
}

/// True when every element of `tail` equals the element `period` positions earlier (wrapping
/// back into the first `period` elements), i.e. `tail` is exactly periodic with period `period`.
fn is_periodic(tail: &[u32], period: usize) -> bool {
    if period == 0 {
        return false;
    }
    tail.iter()
        .enumerate()
        .all(|(index, value)| *value == tail[index % period])
}

/// Truncate a trailing run of `ids` that is exactly periodic with `period`, keeping exactly one
/// copy of the repeating unit. Any non-periodic prefix is left untouched.
///
/// No-op when `period` is zero or larger than `ids`.
pub fn truncate_degenerate_tail(ids: &mut Vec<u32>, period: usize) {
    if period == 0 || period > ids.len() {
        return;
    }
    let mut boundary = ids.len();
    while boundary >= 2 * period {
        let previous_unit = &ids[boundary - 2 * period..boundary - period];
        let last_unit = &ids[boundary - period..boundary];
        if previous_unit == last_unit {
            boundary -= period;
        } else {
            break;
        }
    }
    ids.truncate(boundary);
}

/// Check the trailing [`REPEAT_GUARD_WINDOW`] ids of `ids` for a degenerate period and, if
/// found, truncate the run in place (keeping one copy of the unit) and return the period.
/// Shared by every decode loop that wires the guard in after pushing a new token.
#[must_use]
pub fn stop_if_degenerate(ids: &mut Vec<u32>) -> Option<usize> {
    let period = degenerate_tail_period(ids, REPEAT_GUARD_WINDOW, REPEAT_GUARD_MAX_PERIOD)?;
    truncate_degenerate_tail(ids, period);
    Some(period)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_detect_period_one_run_of_sixty_four_identical_tokens() {
        let ids = vec![7u32; REPEAT_GUARD_WINDOW];
        assert_eq!(
            degenerate_tail_period(&ids, REPEAT_GUARD_WINDOW, REPEAT_GUARD_MAX_PERIOD),
            Some(1)
        );
    }

    #[test]
    fn should_detect_period_three_run_and_truncate_to_prefix_plus_one_unit() {
        let unit = [1u32, 2, 3];
        let mut ids: Vec<u32> = unit.iter().copied().cycle().take(3 * 22).collect();
        assert_eq!(
            degenerate_tail_period(&ids, REPEAT_GUARD_WINDOW, REPEAT_GUARD_MAX_PERIOD),
            Some(3)
        );

        truncate_degenerate_tail(&mut ids, 3);
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn should_not_flag_a_strictly_increasing_sequence() {
        let ids: Vec<u32> = (0..64).collect();
        assert_eq!(
            degenerate_tail_period(&ids, REPEAT_GUARD_WINDOW, REPEAT_GUARD_MAX_PERIOD),
            None
        );
    }

    #[test]
    fn should_not_flag_a_sequence_shorter_than_the_window() {
        let ids = vec![9u32; REPEAT_GUARD_WINDOW - 1];
        assert_eq!(
            degenerate_tail_period(&ids, REPEAT_GUARD_WINDOW, REPEAT_GUARD_MAX_PERIOD),
            None
        );
    }

    #[test]
    fn should_not_flag_a_period_nine_run_because_it_exceeds_max_period() {
        let unit = [1u32, 2, 3, 4, 5, 6, 7, 8, 9];
        let ids: Vec<u32> = unit.iter().copied().cycle().take(9 * 8).collect();
        assert_eq!(
            degenerate_tail_period(&ids, REPEAT_GUARD_WINDOW, REPEAT_GUARD_MAX_PERIOD),
            None
        );
    }

    #[test]
    fn should_truncate_only_the_periodic_tail_and_keep_the_non_periodic_prefix() {
        let prefix: Vec<u32> = (1..=19).collect();
        let unit = [5u32, 6];
        let mut ids = prefix.clone();
        ids.extend(unit.iter().copied().cycle().take(2 * 40));

        assert_eq!(
            degenerate_tail_period(&ids, REPEAT_GUARD_WINDOW, REPEAT_GUARD_MAX_PERIOD),
            Some(2)
        );

        truncate_degenerate_tail(&mut ids, 2);

        let mut expected = prefix;
        expected.extend_from_slice(&unit);
        assert_eq!(ids, expected);
    }

    #[test]
    fn should_report_and_truncate_via_the_combined_helper() {
        let unit = [1u32, 2, 3];
        let mut ids: Vec<u32> = unit.iter().copied().cycle().take(3 * 22).collect();
        assert_eq!(stop_if_degenerate(&mut ids), Some(3));
        assert_eq!(ids, vec![1, 2, 3]);
    }
}
