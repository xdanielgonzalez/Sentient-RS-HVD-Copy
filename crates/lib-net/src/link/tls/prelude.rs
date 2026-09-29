//! Shared import surface for the tls module.

pub use rustls::pki_types::{CertificateDer, ServerName};
pub use rustls::{
    ClientConfig, ClientConnection, Connection, RootCertStore, ServerConfig, ServerConnection,
};
pub use std::io::{Cursor, Read, Write};
pub use std::sync::Arc;

pub use crate::link::tls_identity::{self, TlsIdentity};

pub use super::types::*;
