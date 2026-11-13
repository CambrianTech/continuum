#![cfg(test)]
    use super::*;

    /// what this catches: a capability report that OVERSTATES what this node has.
    ///
    /// The producer took `system_profiler`'s leading number and multiplied by 1024
    /// unconditionally — right for Apple Silicon (which reports GB), wrong by exactly
    /// 1024x on an Intel Mac (which reports MB). Measured 2026-09-05: "1536 MB" on an
    /// Intel UHD 630 was published as `memoryTotalMb: 1572864`, so the weakest node on
    /// the grid advertised 1.5 TB of VRAM — more than every other GPU combined, from a
    /// box that could not run a decode lane at all.
    ///
    /// Both units are asserted because fixing only the MB case by flipping the
    /// multiplier would silently break every Apple Silicon node instead.
    #[test]
    fn vram_units_are_read_not_assumed() {
        assert_eq!(
            vram_string_to_mb("1536 MB"),
            Some(1536),
            "the Intel Mac case: MB must stay MB, not become 1572864"
        );
        assert_eq!(
            vram_string_to_mb("8 GB"),
            Some(8192),
            "the Apple Silicon case: GB must still convert"
        );
    }

    /// what this catches: guessing a multiplier for a unit we do not recognise.
    ///
    /// `None` surfaces as 0 / unknown, which UNDERSTATES. That asymmetry is the whole
    /// point: understating costs throughput, overstating makes a node claim work it
    /// cannot do and the failure lands far from the cause, on a box that "had plenty
    /// of memory". Per `[[no-fallbacks-ever]]` — silence is honest, a guess is a
    /// confident lie.
    #[test]
    fn an_unknown_unit_reports_nothing_rather_than_guessing() {
        assert_eq!(vram_string_to_mb("1536"), None, "no unit is not an implied unit");
        assert_eq!(vram_string_to_mb("4 TB"), None, "an unhandled unit must not be assumed");
        assert_eq!(vram_string_to_mb(""), None);
        assert_eq!(vram_string_to_mb("lots MB"), None, "a non-numeric value is not a size");
    }
