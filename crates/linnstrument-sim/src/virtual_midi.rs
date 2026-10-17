//! OS endpoints carry only firmware MIDI. They never generate notes from contacts.
use crest_linnstrument::MidiParser;
use crest_performance_host::midi_ports::{InputPort, OutputPort};
use std::time::Instant;

pub(crate) struct VirtualMidi {
    input: InputPort,
    output: OutputPort,
    parser: MidiParser,
}
impl VirtualMidi {
    #[cfg(unix)]
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            input: InputPort::virtual_port("LinnStrument Simulator feedback", Instant::now())?,
            output: OutputPort::virtual_port("LinnStrument Simulator performance")?,
            parser: MidiParser::default(),
        })
    }
    #[cfg(not(unix))]
    pub fn new() -> Result<Self, String> {
        Err("virtual MIDI ports require Linux or macOS".into())
    }
    pub fn emit(&mut self, bytes: &[(u64, u8)]) -> Result<(), String> {
        for (_, byte) in bytes {
            if let Some(message) = self.parser.feed(*byte) {
                self.output.send(message.as_bytes())?;
            }
        }
        Ok(())
    }
    pub fn feedback(&self) -> Result<Option<String>, String> {
        self.input.poll().map(|packet| {
            packet.map(|p| {
                format!(
                    "midi {}",
                    p.bytes()
                        .iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            })
        })
    }
    pub fn reset(&mut self) {
        self.output.silence();
        self.parser = MidiParser::default();
    }
}
impl Drop for VirtualMidi {
    fn drop(&mut self) {
        self.output.silence();
    }
}
