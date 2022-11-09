use std::str::FromStr;

use clap::Parser;
use futures::{FutureExt, StreamExt};
use libp2p::{
    identify::{Event as IdentifyEvent, Info as IdentifyInfo},
    identity,
    kad::{BootstrapOk, KademliaEvent, QueryResult},
    swarm::SwarmEvent,
    Multiaddr, PeerId,
};

mod behaviour;
mod transport;

use behaviour::BehaviourEvent;

const BOOTNODES: [&str; 4] = [
    "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
];

#[derive(Debug, Parser)]
#[clap(name = "libp2p test")]
struct Opt {
    #[clap(long)]
    quic: bool,
    #[clap(long)]
    hole_punching: bool,
    #[clap(long)]
    listen_quic_only: bool,
    #[clap(long)]
    query_peer: Option<PeerId>,
    #[clap(long)]
    keep_alive: bool,
    #[clap(long)]
    limit: Option<u32>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    let keypair = identity::Keypair::generate_ed25519();
    let peer_id = keypair.public().to_peer_id();

    println!("Node PeerId: {peer_id}");
    let mut swarm = behaviour::Behaviour::new(keypair, opt.hole_punching, opt.quic, opt.keep_alive, opt.limit)?;

    let bootaddr = Multiaddr::from_str("/dnsaddr/bootstrap.libp2p.io")?;
    for peer in &BOOTNODES {
        swarm
            .behaviour_mut()
            .kademlia
            .add_address(&PeerId::from_str(peer)?, bootaddr.clone());
    }

    if !opt.listen_quic_only {
        swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap())?;
        swarm.listen_on("/ip6/::/tcp/0".parse().unwrap())?;
    }

    if opt.quic {
        swarm.listen_on("/ip4/0.0.0.0/udp/0/quic".parse().unwrap())?;
        swarm.listen_on("/ip6/::/udp/0/quic".parse().unwrap())?;
    }

    let mut bootstrap_interval = tokio::time::interval(std::time::Duration::from_secs(60));
    let mut connection_check = tokio::time::interval(std::time::Duration::from_secs(60));
    let mut bootstrapping = false;
    
    loop {
        tokio::select! {
            _ = connection_check.tick() => {
                println!("Peers connected: {}", swarm.connected_peers().count());
            },
            _ = bootstrap_interval.tick() => {
                if !bootstrapping {
                    println!("Bootstrapping kad");
                    swarm.behaviour_mut().kademlia.bootstrap()?;
                    bootstrapping = true;
                }
            },
            event = swarm.select_next_some().fuse() => {
                match event {
                    SwarmEvent::Behaviour(BehaviourEvent::Identify(IdentifyEvent::Received {
                        peer_id,
                        info: IdentifyInfo {
                                listen_addrs,
                                protocols,
                                ..
                        },
                    })) => {
                            if protocols
                                .iter()
                                .any(|p| p.as_bytes() == libp2p::kad::protocol::DEFAULT_PROTO_NAME)
                            {
                                for addr in &listen_addrs {
                                    swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
                                }
                            }

                            if protocols
                                .iter()
                                .any(|p| p.as_bytes() == libp2p::autonat::DEFAULT_PROTOCOL_NAME)
                            {
                                for addr in &listen_addrs {
                                    swarm.behaviour_mut().autonat.add_server(peer_id, Some(addr.clone()));
                                }
                            }
                    },
                    SwarmEvent::Behaviour(BehaviourEvent::Kademlia(ev)) => {
                        inject_kad(ev, &mut bootstrapping)
                    },
                    _e => {}
                }
            }
        }
    }
}

fn inject_kad(ev: KademliaEvent, bootstrapping: &mut bool) {
    match ev {
        KademliaEvent::OutboundQueryCompleted { result, .. } => match result {
            QueryResult::Bootstrap(Ok(BootstrapOk { num_remaining, .. })) => {
                if num_remaining == 0 && *bootstrapping {
                    println!("Bootstrap is finished!");
                    *bootstrapping = false;
                }
            }
            _e => {}
        },
        _e => {}
    }
}
