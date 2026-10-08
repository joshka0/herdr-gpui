//! Client storage errors keep their path out of display text, which reaches
//! the UI. The warning log is where that path belongs, so debugging a failed
//! file read does not need a debugger.
use herdr_client::StorageOperation;
use std::{error::Error as StdError, path::Path};

/// Log a failed storage step with its operation and path. Errors without a
/// client storage step in their source chain are not logged.
pub(crate) fn warn_storage_failure(context: &str, error: &(dyn StdError + 'static)) {
    let Some((operation, path)) = storage_context(error) else {
        return;
    };
    tracing::warn!(
        category = "storage",
        context,
        ?operation,
        path = %path.display(),
        %error,
        "Client storage step failed"
    );
}

/// The outermost client storage step anywhere in `error`'s source chain.
fn storage_context<'a>(
    error: &'a (dyn StdError + 'static),
) -> Option<(&'a StorageOperation, &'a Path)> {
    std::iter::successors(Some(error), |&error| error.source()).find_map(|error| {
        match error.downcast_ref::<herdr_client::Error>()? {
            herdr_client::Error::Storage {
                operation, path, ..
            } => Some((operation, path.as_path())),
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests;
