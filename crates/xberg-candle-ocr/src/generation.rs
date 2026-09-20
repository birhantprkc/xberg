//! Shared repeat-guard for the greedy decode loops in the candle VLM OCR engines.
//!
//! A page with a dense, visually-repetitive region (a signature block, a ruled table with wide
//! identical cells) can push a plain argmax decoder into a run of tokens that reproduces the
//! same short unit indefinitely instead of terminating on EOS. This module detects an exactly
//! periodic tail and lets the caller stop early instead of burning the rest of the token budget
//! on invented output.

/// Largest period, in tokens, considered a degenerate repeat.
///
// ~keep: #1674's actual failure is a repeated HTML table ROW, not a single repeated cell --
// observed rows tokenise to roughly 15-60 tokens, so a period cap below ~20 (an earlier version
// of this guard used 8) is invisible to exactly the failure this guard exists to catch. The
// reference DeepSeek-OCR implementation bans repeated 20-grams via `no_repeat_ngram_size=20`
// (a logit-level ban needing per-step top-k tracking, which this argmax-only guard avoids); 32
// is chosen over the reference's bare 20 to also cover the long end of the observed row range
// with one period's margin to spare.
pub const REPEAT_GUARD_MAX_PERIOD: usize = 32;

/// Number of trailing token ids examined for an exactly periodic run.
///
// ~keep: sized as 4x REPEAT_GUARD_MAX_PERIOD so a period at the cap (32) still has to repeat at
// least 4 full times before the guard fires -- comfortably above the >=3-repeat bar this was
// reviewed against, and short of that no legitimate multi-row table (2-3 identical short rows,
// period ~6) is at risk of a false positive; see `should_not_flag_three_identical_short_table_rows`
// below. Detection cost is O(window * max_period) = 128 * 32 = 4096 comparisons per decode step,
// negligible next to one transformer forward pass.
pub const REPEAT_GUARD_WINDOW: usize = REPEAT_GUARD_MAX_PERIOD * 4;

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

    /// Build `count` repeats of `unit`, long enough that the result is at least
    /// `REPEAT_GUARD_WINDOW` tokens regardless of how the constants above are tuned.
    fn repeated(unit: &[u32], count: usize) -> Vec<u32> {
        let ids: Vec<u32> = unit.iter().copied().cycle().take(unit.len() * count).collect();
        assert!(
            ids.len() >= REPEAT_GUARD_WINDOW,
            "test fixture must be at least REPEAT_GUARD_WINDOW long, got {} < {}",
            ids.len(),
            REPEAT_GUARD_WINDOW
        );
        ids
    }

    #[test]
    fn should_detect_period_one_run_of_window_identical_tokens() {
        let ids = vec![7u32; REPEAT_GUARD_WINDOW];
        assert_eq!(
            degenerate_tail_period(&ids, REPEAT_GUARD_WINDOW, REPEAT_GUARD_MAX_PERIOD),
            Some(1)
        );
    }

    #[test]
    fn should_detect_period_three_run_and_truncate_to_prefix_plus_one_unit() {
        let unit = [1u32, 2, 3];
        let mut ids = repeated(&unit, REPEAT_GUARD_WINDOW / unit.len() + 2);
        assert_eq!(
            degenerate_tail_period(&ids, REPEAT_GUARD_WINDOW, REPEAT_GUARD_MAX_PERIOD),
            Some(3)
        );

        truncate_degenerate_tail(&mut ids, 3);
        assert_eq!(ids, vec![1, 2, 3]);
    }

    /// #1674's actual failure mode: a repeated HTML table row (observed at roughly 15-60
    /// tokens), here modelled as a 25-token unit -- squarely inside that range and inside
    /// `REPEAT_GUARD_MAX_PERIOD` (32). Must be caught.
    #[test]
    fn should_detect_a_twenty_five_token_repeating_table_row() {
        let unit: [u32; 25] = std::array::from_fn(|index| 100 + index as u32);
        let mut ids = repeated(&unit, REPEAT_GUARD_WINDOW / unit.len() + 2);
        assert_eq!(
            degenerate_tail_period(&ids, REPEAT_GUARD_WINDOW, REPEAT_GUARD_MAX_PERIOD),
            Some(25),
            "a 25-token repeating unit is within REPEAT_GUARD_MAX_PERIOD and must be detected"
        );

        truncate_degenerate_tail(&mut ids, 25);
        assert_eq!(ids, unit.to_vec());
    }

    /// The mirror image of the case above: three genuinely distinct, short, identical table
    /// rows (period ~6) are exactly the kind of legitimate repetition a real financial or
    /// layout table produces, and must not trip the guard just because it repeats a little.
    #[test]
    fn should_not_flag_three_identical_short_table_rows() {
        let unit = [11u32, 12, 13, 14, 15, 16];
        let ids: Vec<u32> = unit.iter().copied().cycle().take(unit.len() * 3).collect();
        assert!(
            ids.len() < REPEAT_GUARD_WINDOW,
            "fixture must be short enough to stay under the window on its own merits"
        );
        assert_eq!(
            degenerate_tail_period(&ids, REPEAT_GUARD_WINDOW, REPEAT_GUARD_MAX_PERIOD),
            None
        );
    }

    /// Documents where the false-positive boundary sits: once identical short rows fill the
    /// whole window (`REPEAT_GUARD_WINDOW / period` copies, 22 rows of period 6 here) the guard
    /// cannot tell a legitimate run from a hallucinated one and collapses it to a single row.
    /// A page with that many byte-identical rows is accepted as the cost of catching #1674.
    #[test]
    fn should_collapse_identical_short_rows_once_they_fill_the_window() {
        let unit = [11u32, 12, 13, 14, 15, 16];
        let rows_filling_window = REPEAT_GUARD_WINDOW.div_ceil(unit.len());
        let mut ids = repeated(&unit, rows_filling_window);
        assert_eq!(
            degenerate_tail_period(
                &ids[..ids.len() - unit.len()],
                REPEAT_GUARD_WINDOW,
                REPEAT_GUARD_MAX_PERIOD
            ),
            None,
            "one row short of the window must still be accepted"
        );
        assert_eq!(stop_if_degenerate(&mut ids), Some(unit.len()));
        assert_eq!(ids, unit.to_vec());
    }

    #[test]
    fn should_not_flag_a_strictly_increasing_sequence() {
        let ids: Vec<u32> = (0..REPEAT_GUARD_WINDOW as u32).collect();
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

    /// A period one step past the cap must stay invisible to the guard -- otherwise
    /// `REPEAT_GUARD_MAX_PERIOD` is not actually bounding anything.
    #[test]
    fn should_not_flag_a_run_one_period_past_the_max_period_cap() {
        let period = REPEAT_GUARD_MAX_PERIOD + 1;
        let unit: Vec<u32> = (1..=period as u32).collect();
        let ids = repeated(&unit, REPEAT_GUARD_WINDOW / unit.len() + 2);
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
        ids.extend(
            unit.iter()
                .copied()
                .cycle()
                .take(unit.len() * (REPEAT_GUARD_WINDOW / unit.len() + 5)),
        );

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
        let mut ids = repeated(&unit, REPEAT_GUARD_WINDOW / unit.len() + 2);
        assert_eq!(stop_if_degenerate(&mut ids), Some(3));
        assert_eq!(ids, vec![1, 2, 3]);
    }
}
