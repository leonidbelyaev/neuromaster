use domain::base::Dname;
use fdg_sim::petgraph::graph::NodeIndex;
use fdg_sim::{ForceGraph, ForceGraphHelper, Simulation, SimulationParameters};
use std::collections::HashMap;
use std::net::IpAddr;

#[derive(Clone, Debug, PartialEq)]
pub struct SimpleHost {
    pub main_addr: IpAddr,
    pub main_hostname: Option<Dname<Vec<u8>>>,
    pub os_guess: Option<OsGuess>,
    pub rtt: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OsGuess {
    Linux(String),
    Freebsd(String),
    Openbsd(String),
    Other(String),
}

impl OsGuess {
    // This works fine if nobody trips LinuxFreeBSDOpenBSD in the nmap database :P
    fn from_string(string: &String) -> OsGuess {
        if string.contains("Linux") {
            OsGuess::Linux(string.to_string())
        } else if string.contains("FreeBSD") {
            OsGuess::Freebsd(string.to_string())
        } else if string.contains("OpenBSD") {
            OsGuess::Openbsd(string.to_string())
        } else {
            OsGuess::Other(string.to_string())
        }
    }
}

impl SimpleHost {
    pub fn from_fullhost(host: &rust_nmap::host) -> Result<Self, Box<dyn std::error::Error>> {
        let status =
            (|| host.status.as_ref()?.state.as_ref())().ok_or("Failed to find host status")?;

        if status.as_str() == "down" {
            return Err("Host is down".into());
        }

        let address_box =
            (|| host.address.as_ref()?.first())().ok_or("Failed to find host address.")?;

        let addrtype = address_box
            .addrtype
            .as_ref()
            .ok_or("Failed to find host address type.")?;

        let addr_str = address_box
            .addr
            .as_ref()
            .ok_or("Failed to find host address.")?;

        let addr = match addrtype.as_str() {
            "ipv4" => IpAddr::V4(addr_str.parse()?),
            "ipv6" => IpAddr::V6(addr_str.parse()?),
            _ => panic!("Unhandled addrtype. Stopping."),
        };

        let hostname = (|| {
            host.hostnames
                .as_ref()?
                .hostname
                .as_ref()?
                .first()?
                .name
                .as_ref()
        })()
        .map(|hostname| {
            Dname::from_chars(hostname.chars()).expect("Could not parse hostname. Stopping.")
        });

        let os = (|| host.os.as_ref()?.osmatch.as_ref()?.first()?.name.as_ref())()
            .map(|os| OsGuess::from_string(os));

        Ok(Self {
            main_addr: addr,
            main_hostname: hostname,
            os_guess: os,
            rtt: None,
        })
    }

    pub fn from_hop(hop: &rust_nmap::hop) -> Result<Self, Box<dyn std::error::Error>> {
        let hostname = hop.host.as_ref().map(|host| {
            Dname::<Vec<u8>>::from_chars(host.chars()).expect("Could not parse hostname. Stopping.")
        });

        Ok(Self {
            main_addr: hop.ipaddr.as_ref().unwrap().parse().unwrap(),
            main_hostname: hostname,
            os_guess: None,
            rtt: None,
        })
    }

    pub fn from_strs(addr: &str, hostname: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            main_addr: addr.parse()?,
            main_hostname: Some(Dname::from_chars(hostname.chars())?),
            os_guess: None,
            rtt: None,
        })
    }

    pub fn set_rtt(host: &mut Self, rtt: f32) {
        host.rtt = Some(rtt);
    }
}

pub fn build_simulation(
    scan: rust_nmap::nmap_run,
) -> Result<Simulation<SimpleHost, ()>, Box<dyn std::error::Error>> {
    let mut map = HashMap::<IpAddr, NodeIndex>::new();
    let mut graph: ForceGraph<SimpleHost, ()> = ForceGraph::default();

    let localhost = SimpleHost::from_strs("127.0.0.1", "localhost")?;
    let localhost_addr = localhost.main_addr;
    insert(&mut map, &mut graph, localhost);

    let host_list = scan.host.as_ref().ok_or("Could not find scan host list.")?;

    for host in host_list {
        let main = SimpleHost::from_fullhost(host);
        if let Err(_) = main {
            continue;
        }
        insert(&mut map, &mut graph, main?);

        let mut origin_addr = localhost_addr;
        for hop in
            (|| host.trace.as_ref()?.hops.as_ref())().expect("Could not find host traceroute hops.")
        {
            let hop_host = SimpleHost::from_hop(hop)?;
            let hop_addr = hop_host.main_addr;

            if map.contains_key(&hop_addr) {
                let origin_index = *map
                    .get(&origin_addr)
                    .ok_or("Could not find origin in map")?;
                let index = *map.get(&hop_addr).unwrap();
                graph.add_edge(origin_index, index, ());
            } else {
                let origin_index = *map
                    .get(&origin_addr)
                    .ok_or("Could not find origin in map")?;
                let index = insert(&mut map, &mut graph, hop_host);
                graph.add_edge(origin_index, index, ());
            }
            origin_addr = hop_addr;
        }
    }

    println!("{graph:#?}");

    fn insert(
        map: &mut HashMap<IpAddr, NodeIndex>,
        graph: &mut ForceGraph<SimpleHost, ()>,
        host: SimpleHost,
    ) -> NodeIndex {
        let name = host.main_addr.to_string();
        let addr = host.main_addr;
        let index = graph.add_force_node(name, host);
        map.insert(addr, index);
        index
    }

    Ok(Simulation::from_graph(
        graph,
        SimulationParameters::new(
            20.0,
            fdg_sim::Dimensions::Three,
            fdg_sim::force::fruchterman_reingold(3.0, 0.975),
        ),
    ))
}
