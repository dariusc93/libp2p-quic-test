use std::str::FromStr;

use futures::{FutureExt, StreamExt};
use libp2p::{
    identify::{Event as IdentifyEvent, Info as IdentifyInfo},
    identity,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let keypair = identity::Keypair::generate_ed25519();
    let peer_id = keypair.public().to_peer_id();

    println!("Node PeerId: {peer_id}");
    let mut swarm = behaviour::Behaviour::new(keypair, true)?;

    let bootaddr = Multiaddr::from_str("/dnsaddr/bootstrap.libp2p.io")?;
    for peer in &BOOTNODES {
        swarm
            .behaviour_mut()
            .kademlia
            .add_address(&PeerId::from_str(peer)?, bootaddr.clone());
    }

    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap())?;
    swarm.listen_on("/ip6/::/tcp/0".parse().unwrap())?;
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic".parse().unwrap())?;
    swarm.listen_on("/ip6/::/udp/0/quic".parse().unwrap())?;

    swarm.behaviour_mut().kademlia.bootstrap()?;
    
    let mut bootstrap_interval = tokio::time::interval(std::time::Duration::from_secs(5 * 60));

    loop {
        tokio::select! {
            _ = bootstrap_interval.tick() => {
                swarm.behaviour_mut().kademlia.bootstrap()?;
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
                    }
                    _e => {}
                }
            }
        }
    }
}
