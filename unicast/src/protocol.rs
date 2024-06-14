use futures::prelude::*;
use libp2p_swarm::StreamProtocol;
use rand::{distributions, prelude::*};
use std::{io, time::Duration};
use web_time::Instant;

pub const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/ipfs/ping/1.0.0");

/// The `Ping` protocol upgrade.
///
/// The ping protocol sends 32 bytes of random data in configurable
/// intervals over a single outbound substream, expecting to receive
/// the same bytes as a response. At the same time, incoming pings
/// on inbound substreams are answered by sending back the received bytes.
///
/// At most a single inbound and outbound substream is kept open at
/// any time. In case of a ping timeout or another error on a substream, the
/// substream is dropped.
///
/// Successful pings report the round-trip time.
///
/// > **Note**: The round-trip time of a ping may be subject to delays induced
/// >           by the underlying transport, e.g. in the case of TCP there is
/// >           Nagle's algorithm, delayed acks and similar configuration options
/// >           which can affect latencies especially on otherwise low-volume
/// >           connections.
#[derive(Default, Debug, Copy, Clone)]
pub(crate) struct Ping;
const PING_SIZE: usize = 32;

/// Sends a ping and waits for the pong.
pub(crate) async fn send_ping<S>(mut stream: S) -> io::Result<(S, Duration)>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	let payload: [u8; PING_SIZE] = thread_rng().sample(distributions::Standard);
	stream.write_all(&payload).await?;
	stream.flush().await?;
	let started = Instant::now();
	let mut recv_payload = [0u8; PING_SIZE];
	stream.read_exact(&mut recv_payload).await?;
	if recv_payload == payload {
		Ok((stream, started.elapsed()))
	} else {
		Err(io::Error::new(
			io::ErrorKind::InvalidData,
			"Ping payload mismatch",
		))
	}
}

/// Waits for a ping and sends a pong.
pub(crate) async fn recv_ping<S>(mut stream: S) -> io::Result<S>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	let mut payload = [0u8; PING_SIZE];
	stream.read_exact(&mut payload).await?;
	stream.write_all(&payload).await?;
	stream.flush().await?;
	Ok(stream)
}
