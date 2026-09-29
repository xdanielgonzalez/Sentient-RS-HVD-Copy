use super::prelude::*;

impl<C: SerialCap> Check<C> for SerialPresent {
    fn id(&self) -> &'static str {
        "serial_present"
    }

    async fn run(&self, ctx: &C) -> Outcome {
        if ctx.serial_present() {
            Outcome::Pass
        } else {
            Outcome::Fail("configured serial device not present")
        }
    }
}
