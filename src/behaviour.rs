use libp2p::identity::Keypair;
use libp2p::{swarm::behaviour::toggle::Toggle, NetworkBehaviour, Swarm};

use libp2p::{
    self,
    autonat::Behaviour as Autonat,
    dcutr::behaviour::Behaviour as DcutrBehaviour,
    identify::{Behaviour as Identify, Config as IdentifyConfig},
    kad::{store::MemoryStore, Kademlia, KademliaConfig},
    mdns::{MdnsConfig, TokioMdns as Mdns},
    ping::Behaviour as Ping,
    relay::v2::client::Client as RelayClient,
};

use libp2p::swarm::keep_alive::Behaviour as KeepAliveBehaviour;

use crate::transport;

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    pub relay_client: Toggle<RelayClient>,
    pub dcutr: Toggle<DcutrBehaviour>,
    pub autonat: Autonat,
    pub keep_alive: KeepAliveBehaviour,
    pub kademlia: Kademlia<MemoryStore>,
    pub identify: Identify,
    pub ping: Ping,
    pub mdns: Toggle<Mdns>,
}

impl Behaviour {
    pub fn new(keypair: Keypair, hole_punching: bool) -> anyhow::Result<Swarm<Self>> {
        let peer_id = keypair.public().to_peer_id();

        let mdns = Some(Mdns::new(MdnsConfig::default())?).into();
        let autonat = Autonat::new(peer_id, Default::default());
        let ping = Ping::default();
        let identify = Identify::new(IdentifyConfig::new("/ipfs/0.1.0".into(), keypair.public()));
        let store = MemoryStore::new(peer_id);
        let kad_config = KademliaConfig::default();
        let kademlia = Kademlia::with_config(peer_id, store, kad_config);
        let keep_alive = KeepAliveBehaviour::default();
        let dcutr = Toggle::from(hole_punching.then_some(DcutrBehaviour::new()));
        let (relay_client, transport) = match hole_punching {
            true => {
                let (transport, client) = RelayClient::new_transport_and_behaviour(peer_id);
                (Toggle::from(Some(client)), Some(transport))
            }
            _ => (Toggle::from(None), None),
        };

        let behaviour = Self {
            mdns,
            relay_client,
            dcutr,
            autonat,
            kademlia,
            identify,
            ping,
            keep_alive,
        };
        let transport = transport::build_transport(keypair, transport)?;

        let swarm = libp2p::swarm::SwarmBuilder::new(transport, behaviour, peer_id)
            .dial_concurrency_factor(16_u8.try_into().unwrap())
            .executor(Box::new(|fut| {
                tokio::spawn(fut);
            }))
            .build();
        Ok(swarm)
    }
}
