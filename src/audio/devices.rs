use anyhow::{Context, Result};
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    pub id: u32,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
struct PipeWireObject {
    id: u32,
    #[serde(rename = "type")]
    object_type: Option<String>,
    info: Option<PipeWireInfo>,
}

#[derive(Debug, Deserialize)]
struct PipeWireInfo {
    props: Option<PipeWireProps>,
}

#[derive(Debug, Deserialize)]
struct PipeWireProps {
    #[serde(rename = "media.class")]
    media_class: Option<String>,
    #[serde(rename = "node.name")]
    node_name: Option<String>,
    #[serde(rename = "node.description")]
    node_description: Option<String>,
}

pub fn list_input_devices() -> Result<Vec<AudioDevice>> {
    let output = Command::new("pw-dump")
        .arg("--no-colors")
        .output()
        .context("failed to run pw-dump")?;

    if !output.status.success() {
        anyhow::bail!("pw-dump exited with status {}", output.status);
    }

    let objects: Vec<PipeWireObject> =
        serde_json::from_slice(&output.stdout).context("failed to parse pw-dump JSON")?;

    let mut devices = objects
        .into_iter()
        .filter_map(|object| {
            let props = object.info?.props?;
            let media_class = props.media_class?;
            if object.object_type.as_deref() != Some("PipeWire:Interface:Node") {
                return None;
            }
            if media_class != "Audio/Source" {
                return None;
            }

            let name = props
                .node_name
                .unwrap_or_else(|| format!("node-{}", object.id));
            if name.ends_with(".monitor") || name.contains("PrivacyVoice") {
                return None;
            }

            let description = props.node_description.unwrap_or_else(|| name.clone());

            Some(AudioDevice {
                id: object.id,
                name,
                description,
            })
        })
        .collect::<Vec<_>>();

    devices.sort_by(|left, right| {
        left.description
            .to_lowercase()
            .cmp(&right.description.to_lowercase())
    });
    devices.dedup_by_key(|device| device.id);

    Ok(devices)
}
