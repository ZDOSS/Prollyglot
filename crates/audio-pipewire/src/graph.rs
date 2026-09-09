use std::collections::{BTreeMap, BTreeSet};

use prollyglot_core::{
    ApplicationSource, CaptureError, CaptureSelection, PlaybackDevice, SourceId, SourceSnapshot,
};
use sha2::{Digest, Sha256};

use crate::identity::ProcessIdentity;

fn opaque_id(kind: &str, identity: &str) -> SourceId {
    let digest = Sha256::digest(identity.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    SourceId::new(format!("pipewire-{kind}:{digest}"))
}

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
        opaque_id("output", &self.name)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Client {
    pub serial: String,
    pub application_id: Option<String>,
    pub name: Option<String>,
    pub process: Option<ProcessIdentity>,
}

#[derive(Clone, Debug)]
pub(crate) struct ApplicationNode {
    pub client: u32,
    pub application_id: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Port {
    pub node: u32,
    pub serial: String,
    pub name: String,
    pub output: bool,
    pub audio: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MonitorPort {
    pub id: u32,
    pub node: u32,
    pub serial: String,
    pub gain: f32,
}

#[derive(Default)]
struct ApplicationGroup {
    name: String,
    instances: BTreeSet<String>,
    nodes: Vec<u32>,
}

#[derive(Default)]
pub(crate) struct Graph {
    pub server_cookie: Option<u32>,
    pub sinks: BTreeMap<u32, Sink>,
    pub default_sink: Option<String>,
    pub clients: BTreeMap<u32, Client>,
    pub applications: BTreeMap<u32, ApplicationNode>,
    pub ports: BTreeMap<u32, Port>,
}

impl Graph {
    pub fn select(&self, selection: &CaptureSelection) -> Result<Sink, CaptureError> {
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

    fn application_groups(&self) -> BTreeMap<String, ApplicationGroup> {
        let mut groups = BTreeMap::<String, ApplicationGroup>::new();
        for (id, node) in &self.applications {
            let Some(client) = self.clients.get(&node.client) else {
                continue;
            };
            let name = node
                .name
                .as_ref()
                .or(client.name.as_ref())
                .map(String::as_str)
                .unwrap_or("Audio application");
            let application_id = node
                .application_id
                .as_ref()
                .or(client.application_id.as_ref());
            let identity = if let Some(application_id) = application_id {
                format!("application:{application_id}")
            } else if let Some(process) = &client.process {
                // The application name distinguishes common runtimes that use
                // the same executable. A title alone never establishes identity.
                format!("executable:{}\nname:{name}", process.executable)
            } else {
                // With no durable identity, offer the live client without
                // matching an unrelated client after it exits. Object serials
                // can be reused after a server restart, so scope them to it.
                let Some(cookie) = self.server_cookie else {
                    continue;
                };
                format!("server:{cookie}\nclient:{}", client.serial)
            };
            let source_id = opaque_id("application", &identity);
            let group = groups.entry(source_id.0).or_default();
            if group.name.is_empty() {
                group.name = name.into();
            }
            group.nodes.push(*id);
            group.instances.insert(
                client
                    .process
                    .as_ref()
                    .map(|process| process.instance.clone())
                    .unwrap_or_else(|| format!("client:{}", client.serial)),
            );
        }
        groups
    }

    pub fn application(
        &self,
        source_id: &SourceId,
    ) -> Result<(String, Vec<MonitorPort>), CaptureError> {
        let group = self
            .application_groups()
            .remove(&source_id.0)
            .ok_or_else(|| {
                CaptureError::SourceUnavailable(
                    "The selected application has no available playback streams.".into(),
                )
            })?;
        if group.instances.len() != 1 {
            return Err(CaptureError::AmbiguousSource(
                "More than one running instance matches the selected application. Close the extra instance or choose another source.".into(),
            ));
        }
        let mut targets = Vec::new();
        for node in group.nodes {
            let ports: Vec<_> = self
                .ports
                .iter()
                .filter(|(_, port)| port.node == node && port.output && port.audio)
                .collect();
            let gain = 1.0 / ports.len().max(1) as f32;
            targets.extend(ports.into_iter().map(|(id, port)| MonitorPort {
                id: *id,
                node,
                serial: port.serial.clone(),
                gain,
            }));
        }
        if targets.len() > 128 {
            return Err(CaptureError::SourceUnavailable(
                "This application has too many simultaneous audio channels. Choose Everything I hear for this source.".into(),
            ));
        }
        Ok((group.name, targets))
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
        let mut applications: Vec<_> = self
            .application_groups()
            .into_iter()
            .map(|(id, group)| ApplicationSource {
                id: SourceId::new(id),
                name: group.name,
                instance_count: group.instances.len() as u32,
                device_ids: Vec::new(),
            })
            .collect();
        applications.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.0.cmp(&b.id.0)));
        SourceSnapshot {
            playback_devices,
            applications,
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

    fn client(serial: &str, instance: &str) -> Client {
        Client {
            serial: serial.into(),
            application_id: None,
            name: Some("Fixture".into()),
            process: Some(ProcessIdentity {
                executable: "/private/fixture".into(),
                instance: instance.into(),
            }),
        }
    }

    fn application(client: u32) -> ApplicationNode {
        ApplicationNode {
            client,
            application_id: Some("org.prollyglot.fixture".into()),
            name: None,
        }
    }

    #[test]
    fn groups_streams_by_application_and_refuses_independent_instances() {
        let mut graph = Graph::default();
        graph.clients.insert(1, client("11", "100:1"));
        graph.clients.insert(2, client("12", "100:1"));
        graph.applications.insert(3, application(1));
        graph.applications.insert(4, application(2));
        let apps = graph.snapshot().applications;
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].instance_count, 1);
        assert!(!apps[0].id.0.contains("private"));
        assert!(graph.application(&apps[0].id).is_ok());
        graph
            .clients
            .get_mut(&2)
            .unwrap()
            .process
            .as_mut()
            .unwrap()
            .instance = "200:1".into();
        assert_eq!(graph.snapshot().applications[0].instance_count, 2);
        assert!(matches!(
            graph.application(&apps[0].id),
            Err(CaptureError::AmbiguousSource(_))
        ));
        graph.applications.remove(&4);
        graph.clients.insert(1, client("30", "300:5"));
        assert_eq!(graph.snapshot().applications[0].id, apps[0].id);
        assert!(graph.application(&apps[0].id).is_ok());
    }

    #[test]
    fn unknown_clients_never_merge_just_because_their_names_match() {
        let mut graph = Graph {
            server_cookie: Some(7),
            ..Default::default()
        };
        for n in 1..=2 {
            graph.clients.insert(
                n,
                Client {
                    serial: n.to_string(),
                    application_id: None,
                    name: Some("Audio player".into()),
                    process: None,
                },
            );
            graph.applications.insert(
                n + 10,
                ApplicationNode {
                    client: n,
                    application_id: None,
                    name: None,
                },
            );
        }
        assert_eq!(graph.snapshot().applications.len(), 2);
        let previous = graph.snapshot().applications;
        graph.server_cookie = Some(8);
        assert!(
            graph
                .snapshot()
                .applications
                .iter()
                .all(|app| previous.iter().all(|old| old.id != app.id))
        );
    }

    #[test]
    fn mixes_every_channel_of_each_selected_stream_without_monitoring_outputs() {
        let mut graph = Graph::default();
        graph.clients.insert(1, client("11", "100:1"));
        graph.applications.insert(3, application(1));
        for (id, node, output) in [(10, 3, true), (11, 3, true), (12, 9, true), (13, 3, false)] {
            graph.ports.insert(
                id,
                Port {
                    node,
                    serial: id.to_string(),
                    name: "channel".into(),
                    output,
                    audio: true,
                },
            );
        }
        let id = &graph.snapshot().applications[0].id;
        let (_, ports) = graph.application(id).unwrap();
        assert_eq!(ports.len(), 2);
        assert!(ports.iter().all(|port| port.node == 3 && port.gain == 0.5));
    }
}
