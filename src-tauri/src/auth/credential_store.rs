// Thin wrapper over the `keyring` crate. On Windows, this resolves to the
// Windows Credential Manager (which encrypts entries with DPAPI under the
// hood). The wizard's Apple ID step will store its session token here; right
// now the module only exposes the primitive API + tests so the foundation is
// in place when that step lands.
//
// API contract: idempotent delete, None-on-missing-entry retrieve. Errors
// other than "entry not found" propagate as AuthError so callers can surface
// them in the UI.

// IPC commands that consume this module land with the Apple ID step; the
// public API is exercised by the test suite in the meantime, so silence the
// dead-code lint at module level rather than peppering #[allow] across every fn.
#![allow(dead_code)]

use keyring::{Entry, Error as KeyringError};
use thiserror::Error;

const SERVICE_NAME: &str = "org.openswiftstudio";

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("keyring: {0}")]
    Keyring(#[from] KeyringError),
}

fn entry_for(service: &str, key: &str) -> Result<Entry, AuthError> {
    Entry::new(service, key).map_err(AuthError::from)
}

/// Persist `value` under `key`. Overwrites any existing entry.
pub fn store(key: &str, value: &str) -> Result<(), AuthError> {
    store_in(SERVICE_NAME, key, value)
}

/// Read the value stored under `key`. Returns `Ok(None)` if no entry exists;
/// `Err` for any other failure (corrupted store, OS API error, etc.).
pub fn retrieve(key: &str) -> Result<Option<String>, AuthError> {
    retrieve_from(SERVICE_NAME, key)
}

/// Remove the entry for `key`. Idempotent — succeeds even if the entry is
/// already gone.
pub fn delete(key: &str) -> Result<(), AuthError> {
    delete_from(SERVICE_NAME, key)
}

/// True iff a value is stored under `key`.
pub fn exists(key: &str) -> Result<bool, AuthError> {
    exists_in(SERVICE_NAME, key)
}

// Internal forms parameterized over service name so tests can use a dedicated
// service prefix without colliding with production credentials.

fn store_in(service: &str, key: &str, value: &str) -> Result<(), AuthError> {
    entry_for(service, key)?.set_password(value)?;
    Ok(())
}

fn retrieve_from(service: &str, key: &str) -> Result<Option<String>, AuthError> {
    match entry_for(service, key)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(e) => Err(AuthError::from(e)),
    }
}

fn delete_from(service: &str, key: &str) -> Result<(), AuthError> {
    match entry_for(service, key)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(KeyringError::NoEntry) => Ok(()),
        Err(e) => Err(AuthError::from(e)),
    }
}

fn exists_in(service: &str, key: &str) -> Result<bool, AuthError> {
    match entry_for(service, key)?.get_password() {
        Ok(_) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(false),
        Err(e) => Err(AuthError::from(e)),
    }
}

#[cfg(test)]
#[cfg(target_os = "windows")]
mod tests {
    use super::*;

    // Dedicated service name keeps test entries isolated from real ones —
    // cleanup via `cmdkey /delete:openswiftstudio.test*` if a test crashes
    // mid-run.
    const TEST_SERVICE: &str = "org.openswiftstudio.test";

    fn unique_key(prefix: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{prefix}-{nanos}")
    }

    #[test]
    fn roundtrip_stores_reads_and_deletes() {
        let key = unique_key("roundtrip");
        let value = "hunter2-very-secret";

        // Pre-state: no entry
        assert_eq!(retrieve_from(TEST_SERVICE, &key).unwrap(), None);
        assert_eq!(exists_in(TEST_SERVICE, &key).unwrap(), false);

        // Store
        store_in(TEST_SERVICE, &key, value).expect("store should succeed");
        assert_eq!(exists_in(TEST_SERVICE, &key).unwrap(), true);
        assert_eq!(
            retrieve_from(TEST_SERVICE, &key).unwrap(),
            Some(value.to_string())
        );

        // Overwrite
        let value2 = "even-more-secret";
        store_in(TEST_SERVICE, &key, value2).expect("overwrite should succeed");
        assert_eq!(
            retrieve_from(TEST_SERVICE, &key).unwrap(),
            Some(value2.to_string())
        );

        // Delete
        delete_from(TEST_SERVICE, &key).expect("delete should succeed");
        assert_eq!(retrieve_from(TEST_SERVICE, &key).unwrap(), None);
        assert_eq!(exists_in(TEST_SERVICE, &key).unwrap(), false);

        // Idempotent delete
        delete_from(TEST_SERVICE, &key).expect("second delete should be ok");
    }

    #[test]
    fn retrieve_returns_none_for_missing_key() {
        let key = unique_key("missing");
        assert_eq!(retrieve_from(TEST_SERVICE, &key).unwrap(), None);
    }

    #[test]
    fn delete_is_ok_when_entry_does_not_exist() {
        let key = unique_key("delete-noop");
        delete_from(TEST_SERVICE, &key).expect("delete on missing entry should be Ok");
    }
}
