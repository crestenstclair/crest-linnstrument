//! Replayable manual runs: commands cross the same worker sensor/MIDI boundary.
use crate::{FirmwareSnapshot, Worker};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

const MAX_BYTES: usize = 32_000_000;
const MAX_STEPS: usize = 50_000;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GestureRun {
    version: u32,
    fixture: serde_json::Value,
    initial: FirmwareSnapshot,
    steps: Vec<Step>,
    #[serde(skip)]
    used_bytes: usize,
    #[serde(skip)]
    full: bool,
    #[serde(skip)]
    previous_leds: Vec<[u8; 3]>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    command: String,
    at_us: u64,
    midi: Vec<(u64, u8)>,
    led_changes: Vec<(usize, [u8; 3])>,
    snapshot_sha256: String,
}
fn digest(snapshot: &FirmwareSnapshot) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(snapshot).expect("integer firmware snapshot serialization")
        )
    )
}
impl GestureRun {
    pub fn new(initial: &FirmwareSnapshot, flash: &Path) -> Result<Self, String> {
        let fixture = serde_json::from_slice(
            &std::fs::read(flash.with_extension("json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            version: 1,
            fixture,
            initial: initial.clone(),
            steps: Vec::new(),
            used_bytes: 0,
            full: false,
            previous_leds: initial.leds.clone(),
        })
    }
    pub fn record(&mut self, command: &str, snapshot: &FirmwareSnapshot) {
        if self.full {
            return;
        }
        let led_changes: Vec<_> = snapshot
            .leds
            .iter()
            .enumerate()
            .filter(|(i, rgb)| self.previous_leds.get(*i) != Some(rgb))
            .map(|(i, rgb)| (i, *rgb))
            .collect();
        // Conservative JSON budget, separate from the strict serialized file cap.
        let bytes = 256 + command.len() + snapshot.midi.len() * 32 + led_changes.len() * 32;
        if self.used_bytes + bytes > MAX_BYTES / 2 || self.steps.len() == MAX_STEPS {
            self.full = true;
            return;
        }
        self.used_bytes += bytes;
        self.previous_leds.clone_from(&snapshot.leds);
        self.steps.push(Step {
            command: command.into(),
            at_us: snapshot.at_us,
            midi: snapshot.midi.clone(),
            led_changes,
            snapshot_sha256: digest(snapshot),
        });
    }
    pub fn steps(&self) -> usize {
        self.steps.len()
    }
    pub fn full(&self) -> bool {
        self.full
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if self.full {
            return Err("gesture run buffer full; reset device to start a new complete run".into());
        }
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("gesture run exceeds 32 MB".into());
        }
        crest_performance_host::storage::atomic_write(path, &bytes)
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("gesture run exceeds 32 MB".into());
        }
        let run: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if run.version != 1 || run.steps.len() > MAX_STEPS {
            return Err("incompatible gesture run".into());
        }
        let mut last = run.initial.at_us;
        for step in &run.steps {
            let verb = step.command.split_whitespace().next().unwrap_or("");
            if ![
                "advance",
                "contact",
                "release",
                "release_all",
                "switch",
                "midi",
                "snapshot",
            ]
            .contains(&verb)
                || step.command.contains(['\n', '\r'])
                || step.command.len() > 65536
                || step.at_us < last
            {
                return Err("invalid gesture run command or ordering".into());
            }
            last = step.at_us;
        }
        Ok(run)
    }
    /// Fresh process, exact command boundaries and virtual execution costs.
    /// Comparison includes firmware bytes, LED output and peripheral counters.
    pub fn verify(&self, executable: &Path, flash: &Path) -> Result<usize, String> {
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(flash.with_extension("json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if manifest != self.fixture {
            return Err("gesture run fixture mismatch".into());
        }
        let (mut worker, initial) = Worker::start(executable, flash, self.initial.model)?;
        if digest(&initial) != digest(&self.initial) {
            return Err("gesture run boot mismatch".into());
        }
        let mut leds = initial.leds;
        for (index, step) in self.steps.iter().enumerate() {
            let observed = worker.request(&step.command)?;
            for (cell, rgb) in &step.led_changes {
                *leds.get_mut(*cell).ok_or("gesture run LED bounds")? = *rgb;
            }
            if digest(&observed) != step.snapshot_sha256
                || observed.midi != step.midi
                || observed.at_us != step.at_us
                || observed.leds != leds
            {
                return Err(format!(
                    "gesture replay diverged at step {index}: {}",
                    step.command
                ));
            }
        }
        Ok(self.steps.len())
    }
}
