//! Transport-neutral TMS Request/Reply core (#324, DDS WP6) — extracted from
//! `tasks::reply_listener` so the MQTT listener (JSON payloads) and the DDS listener
//! (`tasks::dds::request_listener`, XCDR2 payloads via `lib_tms::cdr::decode`) build the IDENTICAL
//! `Reply` from an already-decoded request. Only the wire decode differs per transport; everything
//! here operates on the typed request/reply structs, never on raw bytes.
//!
//! DEVICE ROLE DECISION (locked, see `docs/tms-compliance.md`): this adapter proxies a STOR-role
//! power device that implements no command surface yet, so a well-formed request gets
//! `ReplyCode::NotImplemented` (echoing its key fields) and an undecodable payload gets
//! `ReplyCode::BadRequest` — both transports answer within the 0.1 s budget.

use lib_mapping::schema::{
    ConfigId, EnergyStartStopRequest, Identity, PowerSwitchRequest, Reply, ReplyCode, ReplyStatus,
    String32, INVALID_PORT_NUMBER,
};

use crate::tasks::util::bounded;

/// Which named request topic/reader an inbound message arrived on — shared vocabulary between the
/// MQTT listener's topic match (`reply_listener.rs`) and a future DDS per-reader dispatch.
#[derive(Clone, Copy)]
pub enum RequestKind {
    EnergyStartStop,
    PowerSwitch,
}

pub(crate) fn reason(text: &str) -> String32 {
    bounded(text)
}

/// The `Reply` to an already-decoded [`EnergyStartStopRequest`]: `NotImplemented`, echoing the
/// request's key fields (`requestingDeviceId`, `sequenceId`, `config`); `portNumber` stays
/// `INVALID_PORT_NUMBER` — this request type carries no port.
pub fn energy_start_stop_reply(device_id: &Identity, req: &EnergyStartStopRequest) -> Reply {
    Reply {
        requesting_device_id: req.request_id.requesting_device_id.clone(),
        target_device_id: device_id.clone(),
        config: req.request_id.config,
        port_number: INVALID_PORT_NUMBER,
        request_sequence_id: req.sequence_id,
        status: ReplyStatus {
            code: ReplyCode::NotImplemented,
            reason: reason("not implemented"),
        },
    }
}

/// The `Reply` to an already-decoded [`PowerSwitchRequest`]: `NotImplemented`, echoing the
/// request's key fields including its `portNumber`.
pub fn power_switch_reply(device_id: &Identity, req: &PowerSwitchRequest) -> Reply {
    Reply {
        requesting_device_id: req.request_id.requesting_device_id.clone(),
        target_device_id: device_id.clone(),
        config: req.request_id.config,
        port_number: req.request_id.port_number,
        request_sequence_id: req.sequence_id,
        status: ReplyStatus {
            code: ReplyCode::NotImplemented,
            reason: reason("not implemented"),
        },
    }
}

/// A `BadRequest` reply for an undecodable request (either transport): nothing parsed to echo, so
/// the requesting id is empty, config is `Unknown`, port is `INVALID_PORT_NUMBER`, and the
/// sequence id is 0.
pub fn bad_request(target_device_id: Identity) -> Reply {
    Reply {
        requesting_device_id: Identity::new(),
        target_device_id,
        config: ConfigId::Unknown,
        port_number: INVALID_PORT_NUMBER,
        request_sequence_id: 0,
        status: ReplyStatus {
            code: ReplyCode::BadRequest,
            reason: reason("malformed request payload"),
        },
    }
}
