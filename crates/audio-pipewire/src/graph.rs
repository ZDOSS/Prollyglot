use std::collections::BTreeMap;

use prollyglot_core::{CaptureError, CaptureSelection, PlaybackDevice, SourceId, SourceSnapshot};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sink {
    pub serial: String,
    pub name: String,
    pub description: String,
}

impl Sink {
    pub fn id(&self) -> SourceId {
        // Names survive ordinary node recreation. Registry IDs and serials do
        // not. Keep private hardware identifiers out of frontend diagnostics.
        let digest = Sha256::digest(self.name.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        SourceId::new(format!("pipewire-output:{digest}"))
    }
}

#[derive(Default)]
pub(crate) struct Graph {
    pub sinks: BTreeMap<u32, Sink>,
    pub default_sink: Option<String>,
}

impl Graph {
    pub fn select(&self, selection: &CaptureSelection) -> Result<Sink, CaptureError> {
        if matches!(selection, CaptureSelection::Application { .. }) {
            return Err(CaptureError::SourceUnavailable(
                "Application capture is not yet available on Ubuntu. Choose Everything I hear."
                    .into(),
            ));
        }
        let mut matches = self.sinks.values().filter(|sink| match selection {
            CaptureSelection::SystemDefault => self.default_sink.as_deref() == Some(&sink.name),
            CaptureSelection::SystemOutput { device_id } => sink.id() == *device_id,
            CaptureSelection::Application { .. } => false,
        });
        let sink = matches.next().ok_or_else(|| CaptureError::SourceUnavailable(
            "The selected PipeWire playback device is unavailable. Check the output device and PipeWire/WirePlumber services.".into(),
        ))?;
        if matches.next().is_some() {
            return Err(CaptureError::AmbiguousSource(
                "Duplicate PipeWire output identities; choose another output.".into(),
            ));
        }
        Ok(sink.clone())
    }

    pub fn snapshot(&self) -> SourceSnapshot {
        let mut playback_devices: Vec<_> = self
            .sinks
            .values()
            .map(|sink| PlaybackDevice {
                id: sink.id(),
                name: sink.description.clone(),
                is_default: self.default_sink.as_deref() == Some(&sink.name),
            })
            .collect();
        playback_devices.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.name.cmp(&b.name)));
        playback_devices.dedup_by(|a, b| a.id == b.id);
        SourceSnapshot {
            playback_devices,
            applications: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sink(name: &str, serial: &str) -> Sink {
        Sink {
            name: name.into(),
            serial: serial.into(),
            description: name.into(),
        }
    }

    #[test]
    fn pinned_identity_survives_recreation_and_never_falls_back_to_default() {
        let a = sink("a", "10");
        let selection = CaptureSelection::SystemOutput { device_id: a.id() };
        let mut graph = Graph {
            default_sink: Some("b".into()),
            ..Default::default()
        };
        graph.sinks.insert(20, sink("b", "11"));
        assert!(graph.select(&selection).is_err());
        graph.sinks.insert(21, sink("a", "12"));
        assert_eq!(graph.select(&selection).unwrap().serial, "12");
        assert_eq!(
            graph.select(&CaptureSelection::SystemDefault).unwrap().name,
            "b"
        );
        graph.sinks.insert(22, sink("a", "13"));
        assert!(matches!(
            graph.select(&selection),
            Err(CaptureError::AmbiguousSource(_))
        ));
    }

    #[test]
    fn missing_default_metadata_does_not_choose_an_arbitrary_source() {
        let mut graph = Graph::default();
        graph.sinks.insert(1, sink("speaker", "5"));
        assert!(graph.select(&CaptureSelection::SystemDefault).is_err());
        assert!(graph.snapshot().applications.is_empty());
    }
}
