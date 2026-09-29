use super::prelude::*;

impl Check<HostCtx> for CloudCredsPresent {
    fn id(&self) -> &'static str {
        "cloud_creds"
    }
    async fn run(&self, ctx: &HostCtx) -> Outcome {
        let Some(cloud) = &ctx.cloud else {
            return Outcome::Pass;
        };
        if !cloud.host_present {
            Outcome::Fail("cloud creds: [cloud].host is missing")
        } else if !cloud.username_present {
            Outcome::Fail("cloud creds: [cloud].username is missing")
        } else if !cloud.ca_file_present {
            Outcome::Fail("cloud creds: [cloud].ca_file is missing")
        } else if !cloud.password_present {
            Outcome::Fail("cloud creds: CLOUD_MQTT_PASS is unset or empty")
        } else {
            Outcome::Pass
        }
    }
}
