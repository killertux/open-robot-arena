//! One reliable link to one remote address, over [`crate::frame`] frames.
//!
//! The link is sans-I/O: it is fed the datagrams that arrived and the clock,
//! and it hands back the datagrams to send and the messages that completed.
//! [`crate::session`] owns the socket.
//!
//! What it does:
//! * a three-way `CONNECT` / `CONNECTED` / `CONNECTED` handshake, retried;
//! * guaranteed messages: sequence numbers, in-order delivery, fragmentation
//!   (`NEW_MSG` / `END_MSG`), cumulative + selective acknowledgement
//!   (piggy-backed on data, or a delayed `SACK`), retransmission;
//! * non-guaranteed messages: one frame, delivered on arrival, never resent
//!   (a message larger than a frame goes guaranteed);
//! * keep-alives while idle, a timeout when the peer falls silent or a frame
//!   exhausts its retries while the peer is still talking, and
//!   `HARD_DISCONNECT`.
//!
//! DirectPlay 8 does the same jobs inside `dpnet.dll` (the original only sees
//! its callback messages); the defaults below are ours, not DirectPlay's.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use crate::frame::*;

/// The longest retransmission timeout the round trip can set.
const MAX_RTO: Duration = Duration::from_millis(500);
/// The longest wait between two resends of one frame.
const MAX_BACKOFF: Duration = Duration::from_millis(3200);

/// Timing of a link.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkConfig {
    /// First retransmission timeout.
    pub rto: Duration,
    /// Retransmissions of one frame, each made after the peer was heard from
    /// again without acknowledging it, before the link is declared dead. A
    /// peer that is silent altogether (busy loading a match: DirectPlay acks
    /// from its own threads, our links are pumped by the game loop) is left
    /// to `timeout`.
    pub max_retries: u32,
    /// Send a keep-alive after this long without sending anything.
    pub keepalive: Duration,
    /// Declare the peer gone after this long without hearing from it.
    pub timeout: Duration,
    /// Hold an acknowledgement this long hoping to piggy-back it.
    pub ack_delay: Duration,
    /// Unacknowledged guaranteed frames in flight (at most 32: one SACK mask).
    pub window: usize,
}

impl Default for LinkConfig {
    fn default() -> Self {
        LinkConfig {
            rto: Duration::from_millis(200),
            max_retries: 12,
            keepalive: Duration::from_millis(1000),
            timeout: Duration::from_secs(30),
            ack_delay: Duration::from_millis(20),
            window: 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    /// We sent `CONNECT` and wait for `CONNECTED`.
    Connecting,
    /// The peer sent `CONNECT`; we answered and wait for its `CONNECTED` (or
    /// data, which implies it).
    Accepting,
    Connected,
    Closed(CloseReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// We closed it.
    Local,
    /// The peer sent `HARD_DISCONNECT`.
    Remote,
    /// Silence, or a frame ran out of retries.
    Timeout,
    /// The handshake never finished.
    ConnectFailed,
}

/// A message that came through, and whether it was a session-core message
/// (`USER_1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivered {
    pub core: bool,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
struct InFlight {
    seq: u8,
    command: u8,
    payload: Vec<u8>,
    sent: Instant,
    retries: u32,
    /// Retransmissions made although the peer had been heard from since the
    /// previous one: it is alive and still has not acknowledged the frame.
    strikes: u32,
}

#[derive(Debug)]
pub struct Link {
    pub config: LinkConfig,
    pub state: LinkState,
    session: u32,
    epoch: Instant,
    msg_id: u8,
    last_sent: Instant,
    last_heard: Instant,
    connect_tries: u32,
    // Sending.
    next_seq: u8,
    in_flight: VecDeque<InFlight>,
    queued: VecDeque<(u8, Vec<u8>)>,
    // Receiving.
    next_recv: u8,
    out_of_order: BTreeMap<u8, (u8, Vec<u8>)>,
    partial: Option<(bool, Vec<u8>)>,
    ack_due: Option<Instant>,
    delivered: VecDeque<Delivered>,
    out: Vec<Vec<u8>>,
    /// Round-trip estimate, for the browser and the UI's ping column.
    pub rtt: Option<Duration>,
}

impl Link {
    fn new(config: LinkConfig, state: LinkState, session: u32, now: Instant) -> Link {
        Link {
            config,
            state,
            session,
            epoch: now,
            msg_id: 0,
            last_sent: now,
            last_heard: now,
            connect_tries: 0,
            next_seq: 0,
            in_flight: VecDeque::new(),
            queued: VecDeque::new(),
            next_recv: 0,
            out_of_order: BTreeMap::new(),
            partial: None,
            ack_due: None,
            delivered: VecDeque::new(),
            out: Vec::new(),
            rtt: None,
        }
    }

    /// Start a connection: the `CONNECT` goes out on the next [`Link::poll`].
    pub fn connect(config: LinkConfig, session: u32, now: Instant) -> Link {
        let mut link = Link::new(config, LinkState::Connecting, session, now);
        link.send_control(OP_CONNECT, 0, now);
        link
    }

    /// Answer a peer's `CONNECT`.
    pub fn accept(config: LinkConfig, frame: &Frame, now: Instant) -> Option<Link> {
        let Frame::Control {
            op: OP_CONNECT,
            msg_id,
            session,
            ..
        } = *frame
        else {
            return None;
        };
        let mut link = Link::new(config, LinkState::Accepting, session, now);
        link.send_control(OP_CONNECTED, msg_id, now);
        Some(link)
    }

    fn stamp(&self, now: Instant) -> u32 {
        now.duration_since(self.epoch).as_millis() as u32
    }

    fn send_control(&mut self, op: u8, rsp_id: u8, now: Instant) {
        self.msg_id = self.msg_id.wrapping_add(1);
        let frame = Frame::Control {
            op,
            poll: op == OP_CONNECT,
            msg_id: self.msg_id,
            rsp_id,
            version: PROTOCOL_VERSION,
            session: self.session,
            timestamp: self.stamp(now),
        };
        self.out.push(frame.encode());
        self.last_sent = now;
    }

    pub fn is_connected(&self) -> bool {
        self.state == LinkState::Connected
    }

    pub fn is_closed(&self) -> bool {
        matches!(self.state, LinkState::Closed(_))
    }

    /// Queue a message. Guaranteed messages are sequenced and resent; others
    /// go once (or guaranteed, when they do not fit one frame).
    pub fn send(&mut self, payload: &[u8], core: bool, guaranteed: bool, now: Instant) {
        if self.is_closed() {
            return;
        }
        let user = if core { CMD_USER_1 } else { 0 };
        if !guaranteed && payload.len() <= MAX_PAYLOAD {
            let frame = Frame::Data {
                command: CMD_DATA | CMD_NEW_MSG | CMD_END_MSG | user,
                control: 0,
                seq: self.next_seq,
                next_recv: self.next_recv,
                mask: self.sack_mask(),
                payload: payload.to_vec(),
            };
            if self.is_connected() {
                self.out.push(frame.encode());
                self.last_sent = now;
                self.ack_due = None;
            }
            // Not connected yet: a non-guaranteed message is simply lost, as
            // it would be on the wire.
            return;
        }
        let chunks: Vec<&[u8]> = if payload.is_empty() {
            vec![&[][..]]
        } else {
            payload.chunks(MAX_PAYLOAD).collect()
        };
        let last = chunks.len() - 1;
        for (i, chunk) in chunks.into_iter().enumerate() {
            let mut command = CMD_DATA | CMD_RELIABLE | CMD_SEQUENTIAL | user;
            if i == 0 {
                command |= CMD_NEW_MSG;
            }
            if i == last {
                command |= CMD_END_MSG;
            }
            self.queued.push_back((command, chunk.to_vec()));
        }
        self.pump(now);
    }

    /// Move queued frames into the window and send them.
    fn pump(&mut self, now: Instant) {
        if !self.is_connected() {
            return;
        }
        while self.in_flight.len() < self.config.window.min(32) {
            let Some((command, payload)) = self.queued.pop_front() else {
                break;
            };
            let seq = self.next_seq;
            self.next_seq = self.next_seq.wrapping_add(1);
            let frame = InFlight {
                seq,
                command,
                payload,
                sent: now,
                retries: 0,
                strikes: 0,
            };
            self.transmit(&frame, false, now);
            self.in_flight.push_back(frame);
        }
    }

    fn transmit(&mut self, f: &InFlight, retry: bool, now: Instant) {
        let frame = Frame::Data {
            command: f.command,
            control: if retry { CTL_RETRY } else { 0 },
            seq: f.seq,
            next_recv: self.next_recv,
            mask: self.sack_mask(),
            payload: f.payload.clone(),
        };
        self.out.push(frame.encode());
        self.last_sent = now;
        self.ack_due = None;
    }

    /// Frames received past a gap, as bits after `next_recv`.
    fn sack_mask(&self) -> Option<u32> {
        let mut mask = 0u32;
        for &seq in self.out_of_order.keys() {
            let d = seq.wrapping_sub(self.next_recv);
            if (1..=32).contains(&d) {
                mask |= 1 << (d - 1);
            }
        }
        (mask != 0).then_some(mask)
    }

    /// Process the peer's acknowledgement of everything before `next_recv`
    /// and the frames its mask names.
    fn acknowledge(&mut self, next_recv: u8, mask: Option<u32>, now: Instant) {
        let mut acked_any = false;
        while let Some(front) = self.in_flight.front() {
            if seq_before(front.seq, next_recv) {
                let f = self.in_flight.pop_front().expect("front exists");
                if f.retries == 0 {
                    self.rtt = Some(now.duration_since(f.sent));
                }
                acked_any = true;
            } else {
                break;
            }
        }
        if let Some(mask) = mask {
            self.in_flight.retain(|f| {
                let d = f.seq.wrapping_sub(next_recv);
                !((1..=32).contains(&d) && mask & (1 << (d - 1)) != 0)
            });
        }
        if acked_any {
            self.pump(now);
        }
    }

    /// Feed one datagram from the peer.
    pub fn receive(&mut self, bytes: &[u8], now: Instant) {
        let Some(frame) = Frame::decode(bytes) else {
            return;
        };
        if self.is_closed() {
            return;
        }
        self.last_heard = now;
        match frame {
            Frame::Control { op, msg_id, .. } => match op {
                OP_CONNECT => {
                    // Our CONNECTED was lost: answer again.
                    if self.state == LinkState::Accepting {
                        self.send_control(OP_CONNECTED, msg_id, now);
                    }
                }
                OP_CONNECTED => match self.state {
                    LinkState::Connecting => {
                        self.state = LinkState::Connected;
                        self.send_control(OP_CONNECTED, msg_id, now);
                        self.pump(now);
                    }
                    LinkState::Accepting => {
                        self.state = LinkState::Connected;
                        self.pump(now);
                    }
                    LinkState::Connected => {
                        // The peer's retry of its handshake: confirm again.
                        self.send_control(OP_CONNECTED, msg_id, now);
                    }
                    LinkState::Closed(_) => {}
                },
                OP_HARD_DISCONNECT => {
                    self.state = LinkState::Closed(CloseReason::Remote);
                }
                _ => {}
            },
            Frame::Sack {
                next_recv,
                mask,
                poll,
                ..
            } => {
                if self.state == LinkState::Accepting {
                    self.state = LinkState::Connected;
                }
                self.acknowledge(next_recv, mask, now);
                if poll {
                    self.ack_due = Some(now);
                }
            }
            Frame::Data {
                command,
                control,
                seq,
                next_recv,
                mask,
                payload,
            } => {
                if self.state == LinkState::Accepting {
                    self.state = LinkState::Connected;
                    self.pump(now);
                }
                self.acknowledge(next_recv, mask, now);
                if control & CTL_KEEPALIVE != 0 && payload.is_empty() && command & CMD_RELIABLE == 0
                {
                    return;
                }
                if command & CMD_RELIABLE == 0 {
                    if command & CMD_NEW_MSG != 0 && command & CMD_END_MSG != 0 {
                        self.delivered.push_back(Delivered {
                            core: command & CMD_USER_1 != 0,
                            payload,
                        });
                    }
                    return;
                }
                // Guaranteed: acknowledge, order, reassemble.
                self.ack_due.get_or_insert(now + self.config.ack_delay);
                if seq == self.next_recv {
                    self.accept_in_order(command, payload);
                    while let Some((command, payload)) = self.out_of_order.remove(&self.next_recv) {
                        self.accept_in_order(command, payload);
                    }
                } else if seq_before(self.next_recv, seq) && seq.wrapping_sub(self.next_recv) <= 32
                {
                    self.out_of_order.entry(seq).or_insert((command, payload));
                } else {
                    // A duplicate: acknowledge at once so the sender stops.
                    self.ack_due = Some(now);
                }
            }
        }
    }

    fn accept_in_order(&mut self, command: u8, payload: Vec<u8>) {
        self.next_recv = self.next_recv.wrapping_add(1);
        let core = command & CMD_USER_1 != 0;
        if command & CMD_NEW_MSG != 0 {
            self.partial = Some((core, Vec::new()));
        }
        if let Some((_, buf)) = &mut self.partial {
            buf.extend_from_slice(&payload);
        }
        if command & CMD_END_MSG != 0 {
            if let Some((core, payload)) = self.partial.take() {
                self.delivered.push_back(Delivered { core, payload });
            }
        }
    }

    /// Run the timers; returns the datagrams to send.
    pub fn poll(&mut self, now: Instant) -> Vec<Vec<u8>> {
        match self.state {
            LinkState::Closed(_) => {}
            LinkState::Connecting | LinkState::Accepting => {
                if now.duration_since(self.last_sent) >= self.config.rto {
                    self.connect_tries += 1;
                    // 12 tries are 2.4 s: too short for a host that is busy for a
                    // moment (a loaded machine). Give up only after the tries
                    // *and* the session's 10 s connect patience.
                    if self.connect_tries > self.config.max_retries
                        && (self.state == LinkState::Accepting
                            || now.duration_since(self.last_heard) >= Duration::from_secs(10))
                    {
                        self.state = LinkState::Closed(CloseReason::ConnectFailed);
                    } else if self.state == LinkState::Connecting {
                        self.send_control(OP_CONNECT, 0, now);
                    }
                }
                if now.duration_since(self.last_heard) >= self.config.timeout
                    && self.state == LinkState::Accepting
                {
                    self.state = LinkState::Closed(CloseReason::ConnectFailed);
                }
            }
            LinkState::Connected => {
                if now.duration_since(self.last_heard) >= self.config.timeout {
                    self.state = LinkState::Closed(CloseReason::Timeout);
                    return std::mem::take(&mut self.out);
                }
                // A sample taken while the peer was busy (it acknowledges only
                // when its loop pumps the link: a match loading takes seconds)
                // must not stretch the timeout: a frame the socket dropped
                // would then wait for the next sample's worth of seconds, and
                // a short match was over before the resend (docs/41).
                let rto = self
                    .rtt
                    .map(|r| (r * 3).max(self.config.rto / 4))
                    .unwrap_or(self.config.rto)
                    .min(MAX_RTO);
                let mut resend = Vec::new();
                for (i, f) in self.in_flight.iter_mut().enumerate() {
                    let backoff = (rto * 2u32.pow(f.retries.min(4))).min(MAX_BACKOFF);
                    if now.duration_since(f.sent) >= backoff {
                        f.retries += 1;
                        if self.last_heard > f.sent {
                            f.strikes += 1;
                        }
                        f.sent = now;
                        resend.push(i);
                    }
                }
                if self
                    .in_flight
                    .iter()
                    .any(|f| f.strikes > self.config.max_retries)
                {
                    self.state = LinkState::Closed(CloseReason::Timeout);
                    return std::mem::take(&mut self.out);
                }
                for i in resend {
                    let f = self.in_flight[i].clone();
                    self.transmit(&f, true, now);
                }
                if self.ack_due.is_some_and(|due| now >= due) {
                    self.send_sack(false, now);
                } else if now.duration_since(self.last_sent) >= self.config.keepalive {
                    self.send_sack(true, now);
                }
            }
        }
        std::mem::take(&mut self.out)
    }

    fn send_sack(&mut self, poll: bool, now: Instant) {
        let frame = Frame::Sack {
            poll,
            flags: SACK_RESPONSE,
            retry: 0,
            next_seq: self.next_seq,
            next_recv: self.next_recv,
            timestamp: self.stamp(now),
            mask: self.sack_mask(),
        };
        self.out.push(frame.encode());
        self.last_sent = now;
        self.ack_due = None;
    }

    /// Messages that completed since the last call.
    pub fn take_delivered(&mut self) -> Vec<Delivered> {
        self.delivered.drain(..).collect()
    }

    /// Close: `HARD_DISCONNECT` (sent three times, it is not acknowledged).
    pub fn close(&mut self, now: Instant) -> Vec<Vec<u8>> {
        if !self.is_closed() {
            for _ in 0..3 {
                self.send_control(OP_HARD_DISCONNECT, 0, now);
            }
            self.state = LinkState::Closed(CloseReason::Local);
        }
        std::mem::take(&mut self.out)
    }

    /// Nothing waits to be sent or acknowledged.
    pub fn is_idle(&self) -> bool {
        self.in_flight.is_empty() && self.queued.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two links and a lossy wire between them.
    struct Pair {
        a: Link,
        b: Link,
        now: Instant,
        drop_every: usize,
        count: usize,
    }

    impl Pair {
        fn new(drop_every: usize) -> Pair {
            let now = Instant::now();
            let mut a = Link::connect(LinkConfig::default(), 42, now);
            let first = a.poll(now);
            let connect = Frame::decode(&first[0]).unwrap();
            let b = Link::accept(LinkConfig::default(), &connect, now).unwrap();
            Pair {
                a,
                b,
                now,
                drop_every,
                count: 1,
            }
        }

        /// Drop about one datagram in `drop_every`, pseudo-randomly (a fixed
        /// pattern can starve one frame forever).
        fn lose(&mut self) -> bool {
            self.count = self.count.wrapping_mul(1103515245).wrapping_add(12345);
            self.drop_every != 0 && (self.count >> 16).is_multiple_of(self.drop_every)
        }

        fn run(&mut self, steps: usize) -> (Vec<Delivered>, Vec<Delivered>) {
            let (mut got_a, mut got_b) = (Vec::new(), Vec::new());
            for _ in 0..steps {
                self.now += Duration::from_millis(10);
                for d in self.a.poll(self.now) {
                    if !self.lose() {
                        self.b.receive(&d, self.now);
                    }
                }
                for d in self.b.poll(self.now) {
                    if !self.lose() {
                        self.a.receive(&d, self.now);
                    }
                }
                got_a.extend(self.a.take_delivered());
                got_b.extend(self.b.take_delivered());
            }
            (got_a, got_b)
        }
    }

    #[test]
    fn the_handshake_connects_both_ends() {
        let mut p = Pair::new(0);
        p.run(5);
        assert!(p.a.is_connected());
        assert!(p.b.is_connected());
    }

    #[test]
    fn guaranteed_messages_arrive_once_and_in_order_through_loss() {
        let mut p = Pair::new(3);
        p.run(20);
        assert!(
            p.a.is_connected() && p.b.is_connected(),
            "{:?} {:?}",
            p.a.state,
            p.b.state
        );
        let big: Vec<u8> = (0..5000u32).map(|i| i as u8).collect();
        for i in 0..40u8 {
            p.a.send(&[i], false, true, p.now);
        }
        p.a.send(&big, true, true, p.now);
        let (_, got) = p.run(400);
        let small: Vec<u8> = got
            .iter()
            .filter(|d| !d.core)
            .map(|d| d.payload[0])
            .collect();
        assert_eq!(
            small,
            (0..40).collect::<Vec<_>>(),
            "{:?} {:?}",
            p.a.state,
            p.b.state
        );
        let core: Vec<_> = got.iter().filter(|d| d.core).collect();
        assert_eq!(core.len(), 1);
        assert_eq!(core[0].payload, big, "fragments reassembled");
        assert!(p.a.is_idle(), "everything acknowledged");
    }

    #[test]
    fn non_guaranteed_messages_are_sent_once() {
        let mut p = Pair::new(0);
        p.run(5);
        p.a.send(b"snap", false, false, p.now);
        let (_, got) = p.run(5);
        assert_eq!(got.len(), 1);
        assert!(p.a.is_idle(), "nothing held for a non-guaranteed send");
    }

    #[test]
    fn silence_times_out_and_disconnect_is_seen() {
        let mut p = Pair::new(0);
        p.run(5);
        // a keeps polling but hears nothing.
        let mut now = p.now;
        for _ in 0..400 {
            now += Duration::from_millis(100);
            p.a.poll(now);
        }
        assert_eq!(p.a.state, LinkState::Closed(CloseReason::Timeout));

        let mut p = Pair::new(0);
        p.run(5);
        for d in p.a.close(p.now) {
            p.b.receive(&d, p.now);
        }
        assert_eq!(p.b.state, LinkState::Closed(CloseReason::Remote));
    }

    /// A peer that stops answering altogether (it is loading a match and
    /// nothing pumps its link) keeps the link until the silence timeout,
    /// however many times a guaranteed frame was resent meanwhile; then the
    /// frame arrives.
    #[test]
    fn a_silent_peer_is_left_to_the_timeout_not_to_the_retries() {
        let mut p = Pair::new(0);
        p.run(5);
        p.a.send(b"prepare", false, true, p.now);
        // 8 s of b not polling: a resends far more than `max_retries` times.
        let mut now = p.now;
        let mut held = Vec::new();
        for _ in 0..800 {
            now += Duration::from_millis(10);
            held.extend(p.a.poll(now));
        }
        assert!(p.a.is_connected(), "{:?}", p.a.state);
        p.now = now;
        for d in held {
            p.b.receive(&d, p.now);
        }
        let (_, got) = p.run(20);
        assert!(p.a.is_connected() && p.b.is_connected());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].payload, b"prepare");
        assert!(p.a.is_idle());
        // Past the timeout the silence rule still closes it.
        p.a.send(b"again", false, true, p.now);
        let mut now = p.now;
        for _ in 0..3100 {
            now += Duration::from_millis(10);
            p.a.poll(now);
        }
        assert_eq!(p.a.state, LinkState::Closed(CloseReason::Timeout));
    }

    /// A peer that keeps talking (its inputs, every step) but never
    /// acknowledges a frame (nothing from us reaches it) is dropped on the
    /// retries: the silence rule never fires, since it is heard all along.
    #[test]
    fn a_talking_peer_that_never_acknowledges_runs_out_the_retries() {
        let mut p = Pair::new(0);
        p.run(5);
        // b hears nothing from a, but must not give up on the silence first.
        p.b.config.timeout = Duration::from_secs(3600);
        p.a.send(b"lost", false, true, p.now);
        let mut steps = 0;
        while p.a.is_connected() && steps < 8000 {
            steps += 1;
            p.now += Duration::from_millis(10);
            p.a.poll(p.now); // everything a sends is lost
            p.b.send(b"input", false, false, p.now);
            for d in p.b.poll(p.now) {
                p.a.receive(&d, p.now);
            }
        }
        assert_eq!(
            p.a.state,
            LinkState::Closed(CloseReason::Timeout),
            "{steps}"
        );
    }

    /// Round-trip samples grow when a peer answers late (each can be three
    /// times the last); a long one must not make the next lost frame wait
    /// seconds for its resend.
    #[test]
    fn a_slow_round_trip_does_not_stretch_the_next_resend() {
        let mut p = Pair::new(0);
        p.run(5);
        p.a.rtt = Some(Duration::from_secs(5));
        p.a.send(b"second", false, true, p.now);
        let mut now = p.now;
        let mut resent = false;
        for _ in 0..100 {
            now += Duration::from_millis(10);
            // Everything a sends is lost: the first poll after the send is
            // the frame itself, a later one is the resend.
            if p.a.poll(now).iter().any(|d| {
                matches!(Frame::decode(d), Some(Frame::Data { control, .. }) if control & CTL_RETRY != 0)
            }) {
                resent = true;
                break;
            }
        }
        assert!(resent, "no resend within a second of a slow sample");
    }

    #[test]
    fn idle_links_keep_each_other_alive() {
        let mut p = Pair::new(0);
        p.run(3500); // 35 s, past the timeout
        assert!(p.a.is_connected() && p.b.is_connected());
    }
}
