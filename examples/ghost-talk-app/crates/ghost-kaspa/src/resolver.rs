pub const PUBLIC_WRPC_RESOLVERS: [&str; 16] = [
    "https://eric.kaspa.stream",
    "https://maxim.kaspa.stream",
    "https://sean.kaspa.stream",
    "https://troy.kaspa.stream",
    "https://john.kaspa.red",
    "https://mike.kaspa.red",
    "https://paul.kaspa.red",
    "https://alex.kaspa.red",
    "https://jake.kaspa.green",
    "https://mark.kaspa.green",
    "https://adam.kaspa.green",
    "https://liam.kaspa.green",
    "https://noah.kaspa.blue",
    "https://ryan.kaspa.blue",
    "https://jack.kaspa.blue",
    "https://luke.kaspa.blue",
];

pub fn resolver_query_url(resolver: &str, network: &str) -> Result<String, String> {
    let network = crate::upstream::portal::primitives::NetworkId::parse(network)?.canonical_name();
    Ok(format!("{resolver}/v2/kaspa/{network}/tls/wrpc/borsh"))
}
