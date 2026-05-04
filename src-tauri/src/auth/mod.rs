// Auth + credential storage.
//
// `credential_store` is a thin wrapper over Windows Credential Manager (via
// the `keyring` crate). The Apple ID step of the setup wizard will be the
// first IPC consumer.

pub mod credential_store;
