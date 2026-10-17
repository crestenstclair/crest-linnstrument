//! Session I/O belongs to the host, never to musical processing.
use crest_linnstrument::OutputProfile;
use crest_performance_core::{
    BeatTime, ClockMapping, DestinationId, Error, Result as CoreResult, Session,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
pub const HOST_VERSION: u32 = 2;
pub const DEFAULT_TEMPO_MILLI_BPM: u32 = 120_000;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputRouteConfig {
    pub destination: DestinationId,
    pub profile: OutputProfile,
}

/// Host-owned envelope. The library session stays independently consumable;
/// MIDI destination profiles are backend state, not core musical domain types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "StoredHostDocument")]
pub struct HostDocument {
    pub host_version: u32,
    pub session: Session,
    pub outputs: Vec<OutputRouteConfig>,
    pub tempo_milli_bpm: u32,
}
/// Only the old envelope may omit tempo. Do not silently default a damaged new
/// file or accept a tempo value hidden under an older version label.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredHostDocument {
    host_version: u32,
    session: Session,
    outputs: Vec<OutputRouteConfig>,
    tempo_milli_bpm: Option<u32>,
}
impl TryFrom<StoredHostDocument> for HostDocument {
    type Error = Error;
    fn try_from(stored: StoredHostDocument) -> CoreResult<Self> {
        let tempo_milli_bpm = match (stored.host_version, stored.tempo_milli_bpm) {
            (1, None) => DEFAULT_TEMPO_MILLI_BPM,
            (HOST_VERSION, Some(tempo)) => tempo,
            (1 | HOST_VERSION, _) => {
                return Err(Error::Invalid("tempo field does not match host version"));
            }
            (version, _) => return Err(Error::Version(version)),
        };
        let document = Self {
            host_version: HOST_VERSION,
            session: stored.session.migrate()?,
            outputs: stored.outputs,
            tempo_milli_bpm,
        };
        document.validate()?;
        Ok(document)
    }
}
impl HostDocument {
    pub fn from_session(session: Session) -> CoreResult<Self> {
        let session = session.migrate()?;
        let destinations: std::collections::BTreeSet<_> = session
            .tracks
            .iter()
            .map(|t| t.instrument.destination)
            .collect();
        Ok(Self {
            host_version: HOST_VERSION,
            tempo_milli_bpm: DEFAULT_TEMPO_MILLI_BPM,
            session,
            outputs: destinations
                .into_iter()
                .map(|destination| OutputRouteConfig {
                    destination,
                    profile: OutputProfile::default(),
                })
                .collect(),
        })
    }
    pub fn validate(&self) -> CoreResult<()> {
        if self.host_version != HOST_VERSION {
            return Err(Error::Version(self.host_version));
        }
        ClockMapping::new(0, BeatTime(0), self.tempo_milli_bpm)?;
        self.session.validate()?;
        let destinations: std::collections::BTreeSet<_> = self
            .session
            .tracks
            .iter()
            .map(|t| t.instrument.destination)
            .collect();
        let mut configured = std::collections::BTreeSet::new();
        for route in &self.outputs {
            route.profile.capabilities()?;
            if !configured.insert(route.destination) {
                return Err(Error::Invalid("duplicate output route"));
            }
        }
        if configured != destinations {
            return Err(Error::Invalid(
                "output routes must match session destinations",
            ));
        }
        Ok(())
    }
}
fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(8_000_001)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8_000_000 {
        return Err("session exceeds 8 MB".into());
    }
    Ok(bytes)
}
pub fn load_session(path: &Path) -> Result<Session, String> {
    let session: Session = serde_json::from_slice(&read_bytes(path)?).map_err(|e| e.to_string())?;
    session.migrate().map_err(|e| e.to_string())
}
/// Accept legacy plain library sessions as well as the versioned host envelope.
pub fn load_document(path: &Path) -> Result<HostDocument, String> {
    let value: serde_json::Value =
        serde_json::from_slice(&read_bytes(path)?).map_err(|e| e.to_string())?;
    let document = if value.get("host_version").is_some() {
        serde_json::from_value(value).map_err(|e| e.to_string())?
    } else {
        HostDocument::from_session(serde_json::from_value(value).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
    };
    document.validate().map_err(|e| e.to_string())?;
    Ok(document)
}
pub fn save_document(path: &Path, document: &HostDocument) -> Result<(), String> {
    document.validate().map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(document).map_err(|e| e.to_string())?;
    if bytes.len() > 8_000_000 {
        return Err("session exceeds 8 MB".into());
    }
    atomic_write(path, &bytes)
}
/// Write a new sibling file, sync it, then atomically replace the destination.
/// Failure before rename preserves the previous session. No shared temp names.
pub fn save_session(path: &Path, session: &Session) -> Result<(), String> {
    session.validate().map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(session).map_err(|e| e.to_string())?;
    if bytes.len() > 8_000_000 {
        return Err("session exceeds 8 MB".into());
    }
    atomic_write(path, &bytes)
}
/// Host-side atomic replacement shared by session and simulator trace files.
/// The caller validates its own schema and size limit before writing.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let filename = path
        .file_name()
        .ok_or("session filename required")?
        .to_string_lossy();
    let temporary = path.with_file_name(format!(
        ".{filename}.{}-{}.tmp",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let result = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .and_then(|_| fs::rename(&temporary, path));
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result.map_err(|e| e.to_string())?;
    #[cfg(unix)]
    fs::File::open(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )
    .and_then(|directory| directory.sync_all())
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_or_failed_replacement_preserves_existing_session() {
        let directory = std::env::temp_dir().join(format!("crest-storage-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("session.json");
        let session = Session::default();
        save_session(&path, &session).unwrap();
        let before = fs::read(&path).unwrap();
        let mut invalid = session.clone();
        invalid.version = u32::MAX;
        assert!(save_session(&path, &invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(load_session(&path).unwrap(), session);
        let existing_directory = directory.join("existing-directory");
        fs::create_dir_all(&existing_directory).unwrap();
        assert!(save_session(&existing_directory, &session).is_err());
        assert!(existing_directory.is_dir());
        assert_eq!(
            fs::read_dir(&directory).unwrap().count(),
            2,
            "failed write must clean its temporary file"
        );
        fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(load_session(&path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
