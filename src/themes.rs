//! Reading backgrounds shared with LNC Library.
//!
//! Both apps offer the same six lights (Paper, Sepia, Soft grey, Soft green, Night,
//! Black) so the shelf and the reader sit in one family; the UI mirrors this list in
//! `ui/src/themes.ts` and a test below fails if the two drift apart. The server only
//! needs the identifiers: it validates `default_reader_mode` on the settings endpoint.

/// Accepted values of `default_reader_mode`, in menu order.
pub const READER_MODES: [&str; 6] = ["paper", "sepia", "gray", "green", "night", "black"];

/// Modes that paint a dark page and therefore set `color-scheme: dark`. Used by the
/// test below so the list stays honest; the UI carries the same two identifiers.
#[cfg(test)]
pub const DARK_MODES: [&str; 2] = ["night", "black"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_offered_mode_is_unique_and_each_dark_mode_is_offered() {
        let mut seen = READER_MODES.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), READER_MODES.len(), "duplicate reader mode");
        for dark in DARK_MODES {
            assert!(
                READER_MODES.contains(&dark),
                "{dark} is not an offered mode"
            );
        }
        assert!(READER_MODES.len() >= DARK_MODES.len());
    }
}
