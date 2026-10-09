//! Which copy of a chart is loaded when several copies share one `ChartId`.
//!
//! The caller sorts its copies into one fixed order (kind, source, path) and
//! passes one `intact` flag per copy in that order.
//!
//! Only `bpm` calls this function for now. The game still opens the first copy in
//! its own scan order, so `bpm`'s `loads:` line can differ from the game until the
//! game adopts this rule in its own milestone.

/// Index of the copy to load: the first intact one, or the first copy when
/// none is intact. `None` when there are no copies at all.
pub fn choose_load_index(intact: &[bool]) -> Option<usize> {
    intact
        .iter()
        .position(|&ok| ok)
        .or_else(|| (!intact.is_empty()).then_some(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_intact_copy_wins() {
        assert_eq!(choose_load_index(&[false, true, true]), Some(1));
    }

    #[test]
    fn falls_back_to_the_first_copy_when_none_is_intact() {
        assert_eq!(choose_load_index(&[false, false]), Some(0));
    }

    #[test]
    fn no_copies_no_choice() {
        assert_eq!(choose_load_index(&[]), None);
    }
}
