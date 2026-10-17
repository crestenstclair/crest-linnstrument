use crest_performance_core::*;

#[test]
fn repeated_tempo_boundaries_preserve_fractional_ticks_without_clock_drift() {
    let mut clock = ClockMapping::new(0, BeatTime(17), 120_000).unwrap();
    let mut numerator = 0_u128;
    let mut previous_tempo = 120_000;
    for micros in 1..=4000 {
        numerator += u128::from(previous_tempo) * 960_000;
        let expected = BeatTime(17 + (numerator / 60_000_000_000) as u64);
        assert_eq!(clock.at_micros(micros).unwrap(), expected);
        let tempo = [60_000, 95_333, 120_000][micros as usize % 3];
        clock = clock.with_tempo(micros, tempo).unwrap();
        assert_eq!(clock.at_micros(micros).unwrap(), expected);
        assert_eq!(clock.milli_bpm(), tempo);
        previous_tempo = tempo;
    }
    assert!(matches!(
        clock.with_tempo(3999, 90_000),
        Err(Error::TimeOrder)
    ));
    assert!(clock.with_tempo(4000, 0).is_err());
    assert!(clock.with_tempo(4000, 999_001).is_err());
    assert_eq!(
        clock.at_micros(4000).unwrap().0,
        17 + (numerator / 60_000_000_000) as u64
    );
}

#[test]
fn carried_fraction_detects_overflow_and_tempo_endpoints_remain_valid() {
    let clock = ClockMapping::new(0, BeatTime(u64::MAX - 1), 120_000).unwrap();
    let changed = clock.with_tempo(1, 60_000).unwrap(); // at MAX + 0.92 fractional ticks
    assert_eq!(changed.at_micros(1).unwrap(), BeatTime(u64::MAX));
    assert!(changed.at_micros(2).is_err()); // another 0.96 tick must not be rounded away
    assert!(changed.with_tempo(2, 120_000).is_err());
    for tempo in [1_000, 999_000] {
        let clock = ClockMapping::new(1, BeatTime(2), tempo).unwrap();
        assert_eq!(
            clock.at_micros(60_000_001).unwrap(),
            BeatTime(2 + u64::from(tempo) * 960)
        );
    }
}
