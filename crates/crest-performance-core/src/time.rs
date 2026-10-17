use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// Absolute or relative musical ticks: 960,000 ticks per quarter note.
/// At 120 BPM one tick is about 0.521 microseconds. No implicit quantization.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BeatTime(pub u64);
pub const TICKS_PER_BEAT: u64 = 960_000;
const CLOCK_DENOMINATOR: u128 = 60_000_000_000;

impl BeatTime {
    pub fn checked_add(self, duration: Self) -> Result<Self> {
        self.0
            .checked_add(duration.0)
            .map(Self)
            .ok_or(Error::Invalid("time overflow"))
    }
}

/// Host-owned local clock conversion. Tempo changes establish a new origin;
/// the engine never samples wall time, samples, or animation frames.
#[derive(Debug, Clone, Copy)]
pub struct ClockMapping {
    origin_micros: u64,
    origin_beat: BeatTime,
    fractional_tick: u64,
    milli_bpm: u32,
}
impl ClockMapping {
    pub fn new(origin_micros: u64, origin_beat: BeatTime, milli_bpm: u32) -> Result<Self> {
        if !(1_000..=999_000).contains(&milli_bpm) {
            return Err(Error::Invalid("tempo"));
        }
        Ok(Self {
            origin_micros,
            origin_beat,
            fractional_tick: 0,
            milli_bpm,
        })
    }
    pub fn at_micros(&self, micros: u64) -> Result<BeatTime> {
        self.position(micros).map(|(at, _)| at)
    }
    /// Preserve the exact fractional tick at a tempo boundary, so repeated
    /// changes do not accumulate rounding drift. Returned public positions are
    /// still rounded down to the documented fixed-point tick resolution.
    pub fn with_tempo(self, micros: u64, milli_bpm: u32) -> Result<Self> {
        let (at, fractional_tick) = self.position(micros)?;
        let mut next = Self::new(micros, at, milli_bpm)?;
        next.fractional_tick = fractional_tick;
        Ok(next)
    }
    fn position(&self, micros: u64) -> Result<(BeatTime, u64)> {
        let elapsed = micros
            .checked_sub(self.origin_micros)
            .ok_or(Error::TimeOrder)?;
        let numerator =
            u128::from(elapsed) * u128::from(self.milli_bpm) * u128::from(TICKS_PER_BEAT)
                + u128::from(self.fractional_tick);
        let ticks = u64::try_from(numerator / CLOCK_DENOMINATOR)
            .map_err(|_| Error::Invalid("time overflow"))?;
        Ok((
            self.origin_beat.checked_add(BeatTime(ticks))?,
            (numerator % CLOCK_DENOMINATOR) as u64,
        ))
    }
    pub const fn milli_bpm(self) -> u32 {
        self.milli_bpm
    }
}
