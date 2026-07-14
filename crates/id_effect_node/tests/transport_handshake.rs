//! Transport + handshake semantics: framed delivery, cookie auth
//! (mutual), version refusal, deterministic partitions on the in-memory
//! transport, and TCP loopback parity.

use bytes::Bytes;
use id_effect_node::handshake::Cookie;
use id_effect_node::id::NodeId;
use id_effect_node::session::Session;
use id_effect_node::transport::memory::MemoryNetwork;
use id_effect_node::transport::tcp::TcpTransport;
use id_effect_node::transport::{Transport, TransportError};
use id_effect_node::wire::Frame;
use id_effect_process::NodeName;

fn node_id(name: &str) -> NodeId {
  NodeId::fresh(&NodeName::new(name))
}

#[tokio::test]
async fn memory_transport_delivers_frames_in_order() {
  let net = MemoryNetwork::new();
  let a = net.transport("a");
  let b = net.transport("b");

  let listener = b.listen("b").await.expect("bind");
  let dial = a.connect("b").await.expect("dial");
  let accepted = listener.accept().await.expect("accept");

  for i in 0..10u8 {
    dial.send(Bytes::from(vec![i])).await.expect("send");
  }
  for i in 0..10u8 {
    assert_eq!(accepted.recv().await.expect("recv"), Bytes::from(vec![i]));
  }
}

#[tokio::test]
async fn memory_partition_kills_connections_and_blocks_dials() {
  let net = MemoryNetwork::new();
  let a = net.transport("a");
  let b = net.transport("b");

  let listener = b.listen("b").await.expect("bind");
  let dial = a.connect("b").await.expect("dial");
  let accepted = listener.accept().await.expect("accept");

  dial.send(Bytes::from_static(b"ok")).await.expect("send");
  assert_eq!(
    accepted.recv().await.expect("recv"),
    Bytes::from_static(b"ok")
  );

  net.partition("a", "b");
  assert!(matches!(
    dial.send(Bytes::from_static(b"x")).await,
    Err(TransportError::Closed)
  ));
  assert!(matches!(
    a.connect("b").await,
    Err(TransportError::Connect(_))
  ));

  // Heal: existing connections stay dead, new dials work.
  net.heal("a", "b");
  let dial2 = a.connect("b").await.expect("dial after heal");
  let accepted2 = listener.accept().await.expect("accept after heal");
  dial2.send(Bytes::from_static(b"back")).await.expect("send");
  assert_eq!(
    accepted2.recv().await.expect("recv"),
    Bytes::from_static(b"back")
  );
}

#[tokio::test]
async fn cookie_handshake_authenticates_both_sides() {
  let net = MemoryNetwork::new();
  let a = net.transport("a");
  let b = net.transport("b");
  let cookie = Cookie::new("monster");

  let id_a = node_id("a@test");
  let id_b = node_id("b@test");

  let listener = b.listen("b").await.expect("bind");
  let dial = a.connect("b").await.expect("dial");
  let accept = listener.accept().await.expect("accept");

  let (initiator, responder) = tokio::join!(
    Session::initiate(dial, &id_a, &cookie),
    Session::respond(accept, &id_b, &cookie),
  );
  let initiator = initiator.expect("initiator handshake");
  let responder = responder.expect("responder handshake");

  assert_eq!(initiator.peer(), &id_b);
  assert_eq!(responder.peer(), &id_a);

  // Authenticated frames flow both ways.
  initiator.send(&Frame::Ping(7)).await.expect("send");
  assert_eq!(responder.recv().await.expect("recv"), Frame::Ping(7));
  responder.send(&Frame::Pong(7)).await.expect("send");
  assert_eq!(initiator.recv().await.expect("recv"), Frame::Pong(7));
}

#[tokio::test]
async fn wrong_cookie_is_rejected() {
  let net = MemoryNetwork::new();
  let a = net.transport("a");
  let b = net.transport("b");

  let id_a = node_id("a@test");
  let id_b = node_id("b@test");

  let listener = b.listen("b").await.expect("bind");
  let dial = a.connect("b").await.expect("dial");
  let accept = listener.accept().await.expect("accept");

  let wrong = Cookie::new("wrong");
  let right = Cookie::new("right");
  let (initiator, responder) = tokio::join!(
    Session::initiate(dial, &id_a, &wrong),
    Session::respond(accept, &id_b, &right),
  );
  assert!(initiator.is_err(), "initiator must fail");
  assert!(responder.is_err(), "responder must fail");
}

#[tokio::test]
async fn tcp_transport_round_trips_authenticated_frames() {
  let tcp = TcpTransport;
  let listener = tcp.listen("127.0.0.1:0").await.expect("bind");
  let addr = listener.local_addr();
  let cookie = Cookie::new("tcp-secret");

  let id_a = node_id("a@tcp");
  let id_b = node_id("b@tcp");

  let dial = tcp.connect(&addr).await.expect("dial");
  let accept = listener.accept().await.expect("accept");

  let (initiator, responder) = tokio::join!(
    Session::initiate(dial, &id_a, &cookie),
    Session::respond(accept, &id_b, &cookie),
  );
  let initiator = initiator.expect("initiator");
  let responder = responder.expect("responder");

  initiator
    .send(&Frame::Data(vec![1, 2, 3]))
    .await
    .expect("send");
  assert_eq!(
    responder.recv().await.expect("recv"),
    Frame::Data(vec![1, 2, 3])
  );
}
