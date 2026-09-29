use super::prelude::*;

impl LocationProvider for ProviderKind {
    async fn locate(&mut self) -> Result<LocationFix, &'static str> {
        match self {
            ProviderKind::Manual(p) => p.locate().await,
            ProviderKind::GpsNmea(p) => p.locate().await,
            ProviderKind::ModemManager(p) => p.locate().await,
            ProviderKind::IpGeo(p) => p.locate().await,
        }
    }
}
