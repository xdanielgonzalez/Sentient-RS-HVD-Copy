//! Type definitions for the broker transport seam.

mod broker_conn;
mod broker_listener;
mod recv_outcome;
mod send_outcome;

pub use broker_conn::BrokerConn;
pub use broker_listener::BrokerListener;
pub use recv_outcome::RecvOutcome;
pub use send_outcome::SendOutcome;

#[cfg(feature = "std")]
mod tcp_broker_conn;
#[cfg(feature = "std")]
mod tcp_broker_listener;

#[cfg(feature = "std")]
pub use tcp_broker_conn::TcpBrokerConn;
#[cfg(feature = "std")]
pub use tcp_broker_listener::TcpBrokerListener;

#[cfg(feature = "std")]
mod mock_broker_conn;
#[cfg(feature = "std")]
mod mock_broker_listener;
#[cfg(feature = "std")]
mod mock_recv_event;
#[cfg(feature = "std")]
mod send_fault;
#[cfg(feature = "std")]
mod send_fault_kind;

#[cfg(feature = "std")]
pub use mock_broker_conn::MockBrokerConn;
#[cfg(feature = "std")]
pub use mock_broker_listener::MockBrokerListener;
#[cfg(feature = "std")]
pub use mock_recv_event::MockRecvEvent;
#[cfg(feature = "std")]
pub use send_fault::SendFault;
#[cfg(feature = "std")]
pub use send_fault_kind::SendFaultKind;
