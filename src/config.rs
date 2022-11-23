use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{ErrorKind, Write},
    net::Ipv4Addr,
    path::PathBuf,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// IPv4 address to listen on
    pub listen: Ipv4Addr,
    /// Port to run the service on
    pub port: u16,
    /// Directory to use for storing JSON DB state
    pub data_dir: PathBuf,
    /// Relative path to a valid private key in PEM format
    pub private_key_path: PathBuf,
    /// Activitypub related configuration for the relay
    pub activity_pub: ActivityPubConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: Ipv4Addr::new(0, 0, 0, 0),
            port: 4242,
            data_dir: PathBuf::from("."),
            private_key_path: PathBuf::from("private-key.pem"),
            activity_pub: Default::default(),
        }
    }
}

impl Config {
    /// Try to load our config file if it exists, otherwise write out our
    /// default config and return that.
    ///
    /// Panics if the config file that is present is invalid or if we are unable
    /// to write out our default config.
    pub fn load_or_write_default(path: PathBuf) -> Self {
        match fs::read_to_string(&path) {
            Ok(content) => serde_yaml::from_str(&content)
                .unwrap_or_else(|e| panic!("unable to load config file: {e}")),

            Err(e) if e.kind() == ErrorKind::NotFound => {
                let mut f = File::create(&path)
                    .unwrap_or_else(|e| panic!("to be able to create config file: {e}"));

                let cfg = Self::default();
                f.write_all(
                    serde_yaml::to_string(&cfg)
                        .unwrap_or_else(|e| panic!("unable to parse config: {e}"))
                        .as_bytes(),
                )
                .unwrap_or_else(|e| panic!("unable to write config file: {e}"));

                cfg
            }

            Err(e) => panic!("unable to read config file: {e}"),
        }
    }

    pub fn base_url(&self) -> String {
        format!("{}:{}", self.listen, self.port)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPubConfig {
    /// Used for generating activitypub messages and linking
    /// activitypub identities. It should be an SSL-enabled domain
    /// reachable by HTTPS.
    pub host: String,
    /// Instances that should always be rejected
    pub blocked_instances: Vec<String>,
    /// Whether or not the allow list should be enabled (blocking
    /// anything not on the list)
    pub allow_list: bool,
    /// Instances that should accepted. Only enforced if allowList=true
    pub allowed_instances: Vec<String>,
}

impl Default for ActivityPubConfig {
    fn default() -> Self {
        Self {
            host: String::from("localhost"),
            blocked_instances: Vec::new(),
            allow_list: false,
            allowed_instances: Vec::new(),
        }
    }
}
