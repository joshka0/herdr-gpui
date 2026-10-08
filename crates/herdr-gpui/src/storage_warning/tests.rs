use super::*;
use std::{io, path::PathBuf};

fn storage(path: &str) -> herdr_client::Error {
    herdr_client::Error::Storage {
        operation: StorageOperation::Read,
        path: PathBuf::from(path),
        source: Box::new(io::Error::from(io::ErrorKind::PermissionDenied).into()),
    }
}

#[test]
fn finds_the_storage_step_at_the_top_of_the_chain() {
    let error = storage("/private/endpoints.json");
    assert_eq!(
        storage_context(&error),
        Some((
            &StorageOperation::Read,
            Path::new("/private/endpoints.json")
        ))
    );
}

#[test]
fn finds_the_storage_step_inside_a_wrapping_gui_error() {
    let error = crate::Error::from(storage("/private/wsl.json"));
    assert_eq!(
        storage_context(&error),
        Some((&StorageOperation::Read, Path::new("/private/wsl.json")))
    );
}

#[test]
fn ignores_errors_without_a_storage_step() {
    let error = crate::Error::from(herdr_client::Error::Disconnected);
    assert_eq!(storage_context(&error), None);
}
