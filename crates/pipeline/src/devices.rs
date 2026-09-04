//! Microphone discovery.

use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDevice {
    /// Human-readable name, and the identifier we persist in settings. Names are what
    /// the user recognises, and they survive reboots better than platform device ids.
    pub name: String,
    pub is_default: bool,
    /// The device's own preferred configuration, shown in Debug Mode.
    pub default_sample_rate: Option<u32>,
    pub default_channels: Option<u16>,
}

/// Every input device the default host can see.
///
/// A device that fails to describe itself is skipped with a warning rather than
/// failing the whole list — one broken virtual device should not hide the real mic.
pub fn list_input_devices() -> Result<Vec<InputDevice>> {
    let host = cpal::default_host();

    let default_name = host
        .default_input_device()
        .and_then(|d| d.description().ok())
        .map(|d| d.name().to_string());

    let devices = host
        .input_devices()
        .map_err(|e| Error::Enumeration(e.to_string()))?;

    let mut result = Vec::new();
    for device in devices {
        let name = match device.description() {
            Ok(description) => description.name().to_string(),
            Err(e) => {
                tracing::warn!(error = %e, "skipping an input device that could not be described");
                continue;
            }
        };

        let config = device.default_input_config().ok();
        result.push(InputDevice {
            is_default: Some(&name) == default_name.as_ref(),
            name,
            default_sample_rate: config.as_ref().map(|c| c.sample_rate()),
            default_channels: config.as_ref().map(|c| c.channels()),
        });
    }

    result.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.name.cmp(&b.name)));
    Ok(result)
}

/// Find a device by name, or the system default when `name` is `None`.
pub(crate) fn find_device(name: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();

    match name {
        None => host.default_input_device().ok_or(Error::NoInputDevice),
        Some(wanted) => {
            let devices = host
                .input_devices()
                .map_err(|e| Error::Enumeration(e.to_string()))?;

            for device in devices {
                if let Ok(description) = device.description() {
                    if description.name() == wanted {
                        return Ok(device);
                    }
                }
            }
            Err(Error::DeviceNotFound(wanted.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumeration_succeeds_and_reports_at_most_one_default() {
        let devices = match list_input_devices() {
            Ok(devices) => devices,
            Err(e) => {
                eprintln!("skipping: no audio host available ({e})");
                return;
            }
        };

        let defaults = devices.iter().filter(|d| d.is_default).count();
        assert!(
            defaults <= 1,
            "more than one device claims to be the default"
        );

        for device in &devices {
            assert!(!device.name.is_empty(), "a device reported an empty name");
            if let Some(rate) = device.default_sample_rate {
                assert!((8_000..=192_000).contains(&rate), "implausible rate {rate}");
            }
        }

        // The default device, when there is one, must sort first.
        if devices.iter().any(|d| d.is_default) {
            assert!(devices[0].is_default);
        }
    }

    #[test]
    fn looking_up_a_device_that_does_not_exist_says_so() {
        let err = find_device(Some("Definitely Not A Real Microphone")).expect_err("should fail");
        assert!(
            matches!(err, Error::DeviceNotFound(_)),
            "expected DeviceNotFound, got {err:?}"
        );
    }

    #[test]
    fn every_listed_device_can_be_found_again_by_name() {
        let Ok(devices) = list_input_devices() else {
            return;
        };

        for device in devices {
            assert!(
                find_device(Some(&device.name)).is_ok(),
                "device '{}' was listed but could not be looked up",
                device.name
            );
        }
    }
}
