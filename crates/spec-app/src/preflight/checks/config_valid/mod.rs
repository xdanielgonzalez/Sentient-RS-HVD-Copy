//! Configuration validation check.

mod prelude;
mod types;
pub use types::*;

mod impl_config_valid;

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use crate::preflight::capabilities::ConfigCap;
    use crate::preflight::{Check, Outcome};

    struct FakeCtx {
        valid: bool,
    }

    impl crate::preflight::capabilities::ConfigCap for FakeCtx {
        fn config_check(&self) -> Result<(), &'static str> {
            if self.valid {
                Ok(())
            } else {
                Err("bad config")
            }
        }
    }

    #[test]
    fn id_is_stable() {
        assert_eq!(Check::<FakeCtx>::id(&ConfigValid), "config_valid");
        assert!(FakeCtx { valid: true }.config_check().is_ok());
    }

    #[test]
    fn negative_path_via_config_cap_surfaces_the_check_reason() {
        // Drives the negative path through the `ConfigCap` capability trait (not by constructing
        // `Outcome::Fail` directly), so this actually exercises `ConfigValid::run`'s mapping of
        // `Err(reason)` -> `Outcome::Fail(reason)`.
        let ctx = FakeCtx { valid: false };
        let outcome = crate::preflight::pollster_block_on(ConfigValid.run(&ctx));
        match outcome {
            Outcome::Fail(reason) => assert_eq!(reason, "bad config"),
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[test]
    fn positive_path_via_config_cap() {
        let ctx = FakeCtx { valid: true };
        let outcome = crate::preflight::pollster_block_on(ConfigValid.run(&ctx));
        assert!(matches!(outcome, Outcome::Pass));
    }
}
