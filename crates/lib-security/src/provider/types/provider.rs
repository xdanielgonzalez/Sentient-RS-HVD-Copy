use crate::profile::SecurityProfile;

/// The active security provider. Holds the resolved [`SecurityProfile`]; gates algorithm selection;
/// is the construction point where a FIPS backend runs power-on self-tests.
#[derive(Debug, Clone, Copy)]
pub struct Provider {
    pub(crate) profile: SecurityProfile,
}
