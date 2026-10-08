//! Machines cloud providers created, saved as devices. Herdr's `endpoints.json`
//! has a fixed SSH schema owned upstream, so the GUI keeps these in its own
//! state directory, one list for every provider. Reads and writes are bounded
//! filesystem I/O for background workers only.

use super::{Error, Result};
use herdr_client::{CloudProvider, ConnectTarget};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const FILE: &str = "cloud-devices.json";
const LIMIT: u64 = 256 * 1024;
const MAX_DEVICES: usize = 64;
const LABEL_LIMIT: usize = 128;
const NAME_LIMIT: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedDevice {
    pub(crate) provider: CloudProvider,
    /// The provider's ID for the machine; stable across renames of the label.
    pub(crate) id: String,
    pub(crate) label: String,
    /// The provider account or deployment the machine belongs to.
    pub(crate) account: String,
    /// The provider's name for the machine when it was saved, for display;
    /// connecting goes by `id`, so a rename or a reused name cannot misdirect it.
    pub(crate) machine: String,
    pub(crate) session: String,
    pub(crate) enabled: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    devices: Vec<SavedDevice>,
}

/// Provider IDs and machine names become endpoint IDs and command arguments,
/// so only plain identifier characters are accepted.
fn plain(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

impl SavedDevice {
    pub(crate) fn endpoint_id(&self) -> String {
        format!("{}:{}", self.provider.key(), self.id)
    }

    pub(crate) fn target(&self) -> ConnectTarget {
        ConnectTarget::Cloud {
            provider: self.provider,
            account: self.account.clone(),
            id: self.id.clone(),
            machine: self.machine.clone(),
            session: self.session.clone(),
        }
    }

    fn valid(&self) -> bool {
        plain(&self.id, NAME_LIMIT)
            && plain(&self.machine, NAME_LIMIT)
            && !self.label.trim().is_empty()
            && self.label.len() <= LABEL_LIMIT
            && !self.label.chars().any(char::is_control)
            && !self.account.is_empty()
            && self.account.len() <= 2048
            && !self.account.chars().any(char::is_control)
            && herdr_client::session_socket(Path::new(""), &self.session).is_ok()
    }

    fn same(&self, other: &Self) -> bool {
        self.provider == other.provider && self.id == other.id
    }
}

fn path() -> Result<PathBuf> {
    crate::preferences::state_dir()
        .map(|dir| dir.join(FILE))
        .ok_or(Error::StateDirectory)
}

fn io(path: &Path) -> impl FnOnce(io::Error) -> Error + '_ {
    move |source| Error::Catalog {
        path: path.to_owned(),
        source,
    }
}

fn read(path: &Path) -> Result<Vec<SavedDevice>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io(path)(error)),
    };
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(io(path))?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::Invalid(FILE));
    }
    let document: Document = serde_json::from_slice(&bytes).map_err(Error::Json)?;
    if document.version != 1
        || document.devices.len() > MAX_DEVICES
        || !document.devices.iter().all(SavedDevice::valid)
    {
        return Err(Error::Invalid(FILE));
    }
    Ok(document.devices)
}

fn write(path: &Path, devices: Vec<SavedDevice>) -> Result<()> {
    if devices.len() > MAX_DEVICES || !devices.iter().all(SavedDevice::valid) {
        return Err(Error::Invalid("cloud device"));
    }
    let parent = path.parent().ok_or(Error::StateDirectory)?;
    fs::create_dir_all(parent).map_err(io(parent))?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(io(parent))?;
    serde_json::to_writer_pretty(
        &mut file,
        &Document {
            version: 1,
            devices,
        },
    )
    .map_err(Error::Json)?;
    file.write_all(b"\n").map_err(io(path))?;
    file.as_file().sync_all().map_err(io(path))?;
    file.persist(path).map_err(|error| io(path)(error.error))?;
    Ok(())
}

// Windows each run their own workers; a read-modify-write must not interleave.
fn transaction<T>(work: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    static ACCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ACCESS.lock().unwrap_or_else(|error| error.into_inner());
    work(&path()?)
}

pub(crate) fn load() -> Result<Vec<SavedDevice>> {
    transaction(read)
}

/// Add `device`, or replace the entry with its provider and ID.
pub(crate) fn save(device: SavedDevice) -> Result<()> {
    transaction(|path| save_in(path, device))
}

pub(crate) fn remove(provider: CloudProvider, id: &str) -> Result<()> {
    transaction(|path| {
        let mut devices = read(path)?;
        devices.retain(|saved| !(saved.provider == provider && saved.id == id));
        write(path, devices)
    })
}

fn save_in(path: &Path, device: SavedDevice) -> Result<()> {
    let mut devices = read(path)?;
    match devices.iter_mut().find(|saved| saved.same(&device)) {
        Some(saved) => *saved = device,
        None => devices.push(device),
    }
    write(path, devices)
}

#[cfg(test)]
mod tests;
