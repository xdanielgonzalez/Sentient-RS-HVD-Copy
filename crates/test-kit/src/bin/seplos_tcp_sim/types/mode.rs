use std::path::PathBuf;

pub(crate) enum Mode {
    Connect(String),
    Listen(u16),
    DumpFixtures(PathBuf),
}
