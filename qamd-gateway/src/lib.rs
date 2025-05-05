pub mod actors;
pub mod api;
pub mod config;
pub mod converter;
pub mod error;
pub mod logging;
pub mod ws_server;

pub mod snapshot;
pub mod types;

use actix::prelude::*;

use crate::actors::md_connector::MarketDataConnector;
use crate::actors::md_distributor::MarketDataDistributor;
use crate::config::Config;
use crate::error::GatewayResult;

/// Gateway service for connecting CTP market data to QAMD format with WebSocket distribution
pub struct MdGateway {
    /// Market data connector actor
    md_connector: Addr<MarketDataConnector>,
    /// Market data distributor actor
    md_distributor: Addr<MarketDataDistributor>,
    /// Configuration
    config: Config,
    /// Start time
    start_time: std::time::Instant,
}

impl MdGateway {
    /// Create a new QAMD Gateway instance
    pub fn new(config: Config) -> GatewayResult<Self> {
        // Create the market data distributor actor
        let md_distributor = MarketDataDistributor::default().start();

        // Get broker configurations
        let broker_config = config.get_broker(None)?;
        let broker_configs = vec![broker_config.clone()];

        // QQ broker configuration (if needed)
        let qq_broker_config = config.get_broker(None)?; // Could be configured separately
        let all_broker_configs = vec![broker_config.clone(), qq_broker_config.clone()];

        // Get default subscriptions
        let default_instruments = config.subscription.default_instruments.clone();

        // Create the market data connector actor
        let md_connector = MarketDataConnector::new(
            all_broker_configs,
            default_instruments,
            md_distributor.clone(),
        )
        .start();

        Ok(Self {
            md_connector,
            md_distributor,
            config,
            start_time: std::time::Instant::now(),
        })
    }

    /// Get the market data connector actor
    pub fn get_md_connector(&self) -> &Addr<MarketDataConnector> {
        &self.md_connector
    }

    /// Get the market data distributor actor
    pub fn get_md_distributor(&self) -> &Addr<MarketDataDistributor> {
        &self.md_distributor
    }

    /// Get the configuration
    pub fn get_config(&self) -> &Config {
        &self.config
    }

    /// Get the start time
    pub fn get_start_time(&self) -> std::time::Instant {
        self.start_time
    }
}
