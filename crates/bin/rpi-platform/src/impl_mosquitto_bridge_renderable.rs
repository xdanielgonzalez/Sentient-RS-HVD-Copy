use super::prelude::*;

impl Check<HostCtx> for MosquittoBridgeRenderable {
    fn id(&self) -> &'static str {
        "mosquitto_bridge"
    }
    async fn run(&self, ctx: &HostCtx) -> Outcome {
        match &ctx.cloud {
            None => Outcome::Pass,
            Some(cloud) if cloud.renderable() => Outcome::Pass,
            Some(_) => Outcome::Warn(
                "cloud configured but --emit-mosquitto-bridge would fail closed on missing credentials",
            ),
        }
    }
}
