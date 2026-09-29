//! Check that the configured serial device is present.

mod prelude;
mod types;

pub use types::*;

mod impl_serial_present;

#[cfg(test)]
use prelude::*;

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    struct FakeCtx {
        present: bool,
    }

    impl SerialCap for FakeCtx {
        fn serial_present(&self) -> bool {
            self.present
        }
        fn serial_label(&self) -> &str {
            "fake0"
        }
    }

    #[test]
    fn ids_and_labels() {
        assert_eq!(Check::<FakeCtx>::id(&SerialPresent), "serial_present");
        assert_eq!(FakeCtx { present: true }.serial_label(), "fake0");
    }

    #[test]
    fn fail_path_reason_string() {
        let ctx = FakeCtx { present: false };
        let outcome = crate::preflight::pollster_block_on(SerialPresent.run(&ctx));
        match outcome {
            Outcome::Fail(reason) => {
                assert_eq!(reason, "configured serial device not present");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[test]
    fn pass_path_when_present() {
        let ctx = FakeCtx { present: true };
        let outcome = crate::preflight::pollster_block_on(SerialPresent.run(&ctx));
        assert!(matches!(outcome, Outcome::Pass));
    }
}
