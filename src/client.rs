use ffsend_api::client::{Client, ClientConfigBuilder};

/// Build an ffsend HTTP client with default config.
pub fn build() -> Client {
    ClientConfigBuilder::default()
        .build()
        .expect("Failed to build client config")
        .client(true)
}
