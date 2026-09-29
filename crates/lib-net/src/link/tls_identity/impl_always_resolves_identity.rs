use super::prelude::*;

impl ResolvesServerCert for AlwaysResolvesIdentity {
    fn resolve(&self, _client_hello: ClientHello) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }
}
