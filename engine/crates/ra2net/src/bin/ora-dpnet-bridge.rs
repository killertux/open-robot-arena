//! `ora-dpnet-bridge [LISTEN] [--bind IP] [--hosts A:P,...] [--dump FILE]`: the session side
//! of `tools/dpnet` (docs/38 §14). The replacement `dpnet.dll` in the
//! original's Wine prefix connects here (default 127.0.0.1:27160, the DLL's
//! `ORA_DPNET_BRIDGE`); this runs its sessions on the LAN with the port's
//! transport.

use std::net::TcpListener;

use ra2net::bridge::{self, Bridge};

fn main() {
    let mut listen = bridge::DEFAULT_ADDR.to_string();
    let mut bind = None;
    let mut hosts = Vec::new();
    let mut dump = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--bind" => bind = args.next(),
            "--dump" => dump = args.next(),
            "--hosts" => {
                hosts = args
                    .next()
                    .unwrap_or_default()
                    .split(',')
                    .filter_map(|h| h.parse().ok())
                    .collect()
            }
            "-h" | "--help" => {
                eprintln!(
                    "usage: ora-dpnet-bridge [LISTEN] [--bind IP] [--hosts A:P,...] [--dump FILE]"
                );
                return;
            }
            _ => listen = a,
        }
    }
    let listener = match TcpListener::bind(&listen) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ora-dpnet-bridge: cannot listen on {listen}: {e}");
            std::process::exit(2);
        }
    };
    eprintln!("ora-dpnet-bridge: listening on {listen}");
    let make = || {
        let mut b = Bridge::new();
        if let Some(ip) = bind.as_deref().and_then(|s| s.parse().ok()) {
            b.bind_ip = ip;
        }
        b.extra_hosts = hosts.clone();
        if let Some(path) = &dump {
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                Ok(f) => b.dump = Some(Box::new(f)),
                Err(e) => eprintln!("ora-dpnet-bridge: cannot write {path}: {e}"),
            }
        }
        b
    };
    let start = std::time::Instant::now();
    let log = |line: &str| eprintln!("[{:8.3}] {line}", start.elapsed().as_secs_f64());
    if let Err(e) = bridge::serve(listener, make, log, || false) {
        eprintln!("ora-dpnet-bridge: {e}");
        std::process::exit(1);
    }
}
