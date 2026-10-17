//! Optional OS transport. Core and mandatory headless tests have no MIDI backend.
#[cfg(unix)]
use midir::os::unix::{VirtualInput, VirtualOutput};
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::Instant,
};

const PACKET_BYTES: usize = 1024;
const QUEUED_PACKETS: usize = 256;

pub struct Packet {
    /// Native callback arrival, relative to the supplied host clock origin.
    pub received_us: u64,
    bytes: [u8; PACKET_BYTES],
    len: usize,
}
impl Packet {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
struct InboxSender {
    sender: SyncSender<Packet>,
    failed: Arc<AtomicBool>,
    origin: Instant,
}
impl InboxSender {
    fn receive(&self, bytes: &[u8]) {
        if bytes.len() > PACKET_BYTES || self.failed.load(Ordering::Acquire) {
            self.failed.store(true, Ordering::Release);
            return;
        }
        let mut packet = Packet {
            received_us: self.origin.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
            bytes: [0; PACKET_BYTES],
            len: bytes.len(),
        };
        packet.bytes[..bytes.len()].copy_from_slice(bytes);
        if self.sender.try_send(packet).is_err() {
            self.failed.store(true, Ordering::Release);
        }
    }
}
struct Inbox {
    receiver: Receiver<Packet>,
    failed: Arc<AtomicBool>,
}
impl Inbox {
    fn new(origin: Instant) -> (Self, InboxSender) {
        let (sender, receiver) = mpsc::sync_channel(QUEUED_PACKETS);
        let failed = Arc::new(AtomicBool::new(false));
        (
            Self {
                receiver,
                failed: Arc::clone(&failed),
            },
            InboxSender {
                sender,
                failed,
                origin,
            },
        )
    }
    fn poll(&self) -> Result<Option<Packet>, String> {
        if self.failed.load(Ordering::Acquire) {
            return Err("MIDI input overflow; disconnect and release all voices".into());
        }
        match self.receiver.try_recv() {
            Ok(packet) => Ok(Some(packet)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err("MIDI input disconnected".into()),
        }
    }
}

#[derive(Debug)]
pub struct Port {
    pub id: String,
    pub name: String,
}
pub fn input_ports() -> Result<Vec<Port>, String> {
    let input = MidiInput::new("Crest port inventory").map_err(|e| e.to_string())?;
    input
        .ports()
        .iter()
        .map(|port| {
            Ok(Port {
                id: port.id(),
                name: input.port_name(port).map_err(|e| e.to_string())?,
            })
        })
        .collect()
}
pub fn output_ports() -> Result<Vec<Port>, String> {
    let output = MidiOutput::new("Crest port inventory").map_err(|e| e.to_string())?;
    output
        .ports()
        .iter()
        .map(|port| {
            Ok(Port {
                id: port.id(),
                name: output.port_name(port).map_err(|e| e.to_string())?,
            })
        })
        .collect()
}

pub struct InputPort {
    _connection: MidiInputConnection<InboxSender>,
    inbox: Inbox,
}
impl InputPort {
    /// IDs come from input_ports(); reconnect explicitly resolves the current port.
    pub fn connect(id: &str, origin: Instant) -> Result<Self, String> {
        let mut input = MidiInput::new("Crest performance input").map_err(|e| e.to_string())?;
        input.ignore(Ignore::None);
        let port = input
            .find_port_by_id(id)
            .ok_or("MIDI input port not found")?;
        let (inbox, sender) = Inbox::new(origin);
        let connection = input
            .connect(
                &port,
                "Crest performance input",
                |_, bytes, sender| sender.receive(bytes),
                sender,
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            _connection: connection,
            inbox,
        })
    }
    #[cfg(unix)]
    pub fn virtual_port(name: &str, origin: Instant) -> Result<Self, String> {
        let mut input = MidiInput::new(name).map_err(|e| e.to_string())?;
        input.ignore(Ignore::None);
        let (inbox, sender) = Inbox::new(origin);
        let connection = input
            .create_virtual(name, |_, bytes, sender| sender.receive(bytes), sender)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            _connection: connection,
            inbox,
        })
    }
    pub fn poll(&self) -> Result<Option<Packet>, String> {
        self.inbox.poll()
    }
}
pub struct OutputPort {
    connection: MidiOutputConnection,
}
impl OutputPort {
    pub fn connect(id: &str) -> Result<Self, String> {
        let output = MidiOutput::new("Crest performance output").map_err(|e| e.to_string())?;
        let port = output
            .find_port_by_id(id)
            .ok_or("MIDI output port not found")?;
        let connection = output
            .connect(&port, "Crest performance output")
            .map_err(|e| e.to_string())?;
        Ok(Self { connection })
    }
    #[cfg(unix)]
    pub fn virtual_port(name: &str) -> Result<Self, String> {
        let output = MidiOutput::new(name).map_err(|e| e.to_string())?;
        let connection = output.create_virtual(name).map_err(|e| e.to_string())?;
        Ok(Self { connection })
    }
    pub fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.connection.send(bytes).map_err(|e| e.to_string())
    }
    /// Best-effort emergency cleanup after a failed transport/processing cycle.
    /// Try every channel even if an earlier send failed.
    pub fn silence(&mut self) {
        for channel in 0..16 {
            for control in [64, 123, 120] {
                let _ = self.send(&[0xb0 | channel, control, 0]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overflow_fails_closed_even_if_release_was_dropped() {
        let (inbox, sender) = Inbox::new(Instant::now());
        for _ in 0..QUEUED_PACKETS {
            sender.receive(&[0x91, 60, 90]);
        }
        sender.receive(&[0x81, 60, 0]);
        assert!(inbox.poll().is_err());
    }
    #[test]
    fn packet_bytes_and_arrival_survive_callback() {
        let (inbox, sender) = Inbox::new(Instant::now());
        sender.receive(&[0xe1, 42, 65, 0xf8]);
        let packet = inbox.poll().unwrap().unwrap();
        assert_eq!(packet.bytes(), &[0xe1, 42, 65, 0xf8]);
        assert!(inbox.poll().unwrap().is_none());
        sender.receive(&[0; PACKET_BYTES + 1]);
        assert!(inbox.poll().is_err());
    }
    #[cfg(unix)]
    #[test]
    #[ignore = "requires OS MIDI service; run explicitly on a developer machine"]
    fn virtual_ports_round_trip_through_os() {
        let origin = Instant::now();
        let suffix = std::process::id();
        let input_name = format!("Crest acceptance input {suffix}");
        let output_name = format!("Crest acceptance output {suffix}");
        let input = InputPort::virtual_port(&input_name, origin).unwrap();
        let mut output = OutputPort::virtual_port(&output_name).unwrap();
        // Endpoint discovery may be asynchronous in the system MIDI service.
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        let (input_id, output_id) = loop {
            let i = input_ports()
                .unwrap()
                .into_iter()
                .find(|p| p.name.contains(&output_name));
            let o = output_ports()
                .unwrap()
                .into_iter()
                .find(|p| p.name.contains(&input_name));
            if let (Some(i), Some(o)) = (i, o) {
                break (i.id, o.id);
            }
            assert!(
                Instant::now() < deadline,
                "virtual endpoint discovery timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let receiving_host = InputPort::connect(&input_id, origin).unwrap();
        let mut sending_host = OutputPort::connect(&output_id).unwrap();
        output.send(&[0x91, 60, 90]).unwrap();
        sending_host.send(&[0xb0, 22, 5]).unwrap();
        fn receive(port: &InputPort) -> Vec<u8> {
            let deadline = Instant::now() + std::time::Duration::from_secs(2);
            loop {
                if let Some(packet) = port.poll().unwrap() {
                    return packet.bytes().to_vec();
                }
                assert!(Instant::now() < deadline, "MIDI delivery timed out");
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        assert_eq!(receive(&receiving_host), [0x91, 60, 90]);
        assert_eq!(receive(&input), [0xb0, 22, 5]);
    }
}
