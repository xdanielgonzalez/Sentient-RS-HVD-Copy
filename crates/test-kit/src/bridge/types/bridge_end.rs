use lib_net::channel::WireFrame;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// One direction of a bridged link: a shared, unbounded queue of frames (mirrors the existing
/// single-ended `TestChan` pattern in `lib-link`'s own unit tests, just split across two
/// independent queues so two distinct devices exchange frames instead of one device looping back
/// to itself). Implements both [`lib_net::channel::Sender`] and [`lib_net::channel::Receiver`] over the same underlying queue —
/// cloning it and handing one clone to each side of a direction's two endpoints makes them
/// exchange frames through it.
#[derive(Clone)]
pub struct BridgeEnd(pub Rc<RefCell<VecDeque<WireFrame>>>);
