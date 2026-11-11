//! Provider adapters (P2). One module per provider; every one goes through
//! `connectors::run`, which supplies the hardened fetcher and enforces the gates.
//!
//! None of these has been run against the live provider: all tests use loopback mock
//! servers built from the provider's published response shapes. See
//! `.docs/live-acceptance.md` for the separate, owner-approved acceptance run.

pub mod coingecko;
pub mod ecb;
pub mod edgar_filings;
pub mod federal_register;
pub mod fred;
pub mod mempool;
pub mod sec_edgar;
#[cfg(test)]
pub(crate) mod tests_support;

/// Every connector that ships, for registration and the admin health card.
pub fn all() -> Vec<Box<dyn crate::connectors::Connector>> {
    vec![
        Box::new(mempool::Mempool),
        Box::new(ecb::Ecb),
        Box::new(federal_register::FederalRegister),
        Box::new(fred::Fred),
        Box::new(coingecko::CoinGecko),
        Box::new(sec_edgar::SecEdgar),
        Box::new(edgar_filings::EdgarFilings),
    ]
}
