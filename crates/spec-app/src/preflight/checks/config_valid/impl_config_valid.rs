use super::prelude::*;

impl<C: ConfigCap> Check<C> for ConfigValid {
    fn id(&self) -> &'static str {
        "config_valid"
    }

    async fn run(&self, ctx: &C) -> Outcome {
        match ctx.config_check() {
            Ok(()) => Outcome::Pass,
            Err(msg) => Outcome::Fail(msg),
        }
    }
}
