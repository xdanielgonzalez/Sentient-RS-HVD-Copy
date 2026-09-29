use super::prelude::*;

impl CloudCheckInfo {
    /// True iff every field the bridge render path requires is present.
    pub(crate) fn renderable(&self) -> bool {
        self.host_present && self.username_present && self.ca_file_present && self.password_present
    }
}
