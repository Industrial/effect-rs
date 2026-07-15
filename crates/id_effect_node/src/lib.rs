//! # id_effect_node — distribution layer of the Process Mesh
//!
//! Node identity, cookie-authenticated handshake, pluggable transports,
//! SWIM membership, remote processes, process groups, and distributed
//! supervision (ADR 0009).

#![deny(missing_docs)]

pub mod control;
pub mod discovery;
pub mod dist_supervisor;
pub mod envelope;
pub mod fanout;
pub mod groups;
pub mod handshake;
pub mod id;
pub mod membership;
pub mod mesh;
pub mod registry;
pub mod session;
pub mod singleton;
pub mod transport;
pub mod wire;

pub use control::{
  ClusterStatus, DrainRequest, MemberView, ProcessView, RebalanceRequest, TreeNodeView,
};
pub use discovery::{ChainedDiscovery, Discovery, DnsDiscovery, StaticDiscovery};
pub use dist_supervisor::{DistChildSpec, DistributedSupervisor};
pub use envelope::{
  ExitReasonWire, PidWire, ProcessEnvelope, RemoteFault, TraceContext, decode_envelope,
  empty_trace, encode_envelope, trace_pair,
};
pub use fanout::{SharedGroups, fanout_cast, fanout_cast_on_node};
pub use groups::{Dot, OrSwot, ProcessGroups};
pub use handshake::{Cookie, HandshakeError};
pub use id::NodeId;
pub use membership::{
  Member, Membership, MembershipError, MembershipEvent, NodeMonitors, Output, TimerWheel,
};
pub use mesh::{Mesh, MeshFabric, PeerSender};
pub use registry::{BehaviorEntry, BehaviorRegistry};
pub use session::{Session, SessionError};
#[cfg(feature = "pg")]
pub use singleton::PgAdvisoryLease;
pub use singleton::{
  ClusterSingleton, MemoryLease, SingletonError, SingletonLease, advisory_lock_keys,
};
pub use transport::memory::{InMemoryTransport, MemoryNetwork};
pub use transport::tcp::TcpTransport;
pub use transport::{Connection, Listener, Transport, TransportError};
pub use wire::{Frame, PROTOCOL_VERSION, WireError};
