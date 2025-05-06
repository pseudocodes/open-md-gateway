use crate::error::{GatewayError, GatewayResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// CTP Broker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    /// Broker name
    pub name: String,
    /// Front address (e.g., "tcp://180.168.146.187:10131")
    pub front_addr: String,
    /// Dynlib path (e.g., "../xxx.so")
    pub dynlib_path: String,
    /// User ID
    #[serde(default)]
    pub flow_path: String,
    #[serde(default)]
    pub user_id: String,
    /// Password
    #[serde(default)]
    pub password: String,
    /// Broker ID
    #[serde(default)]
    pub broker_id: String,
    /// App ID
    #[serde(default)]
    pub app_id: String,
    /// Auth code
    #[serde(default)]
    pub auth_code: String,
    /// Source type
    pub source_type: Option<String>,
}

/// WebSocket server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// Hostname to bind (e.g., "0.0.0.0" for all interfaces)
    pub host: String,
    /// Port to bind
    pub port: u16,
    /// Path for the WebSocket endpoint
    pub path: String,
}

/// REST API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestApiConfig {
    /// Hostname to bind
    pub host: String,
    /// Port to bind
    pub port: u16,
    /// CORS settings
    #[serde(default)]
    pub cors: CorsConfig,
}

/// CORS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    /// Allow all origins
    #[serde(default)]
    pub allow_all: bool,
    /// Allowed origins
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// Allow credentials
    #[serde(default)]
    pub allow_credentials: bool,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allow_all: false,
            allowed_origins: vec![],
            allow_credentials: false,
        }
    }
}

/// Subscription settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionConfig {
    /// Default instruments to subscribe on startup
    #[serde(default)]
    pub default_instruments: Vec<String>,
    /// Auto-subscribe to certain instruments based on patterns
    #[serde(default)]
    pub auto_subscribe_patterns: Vec<String>,
}

/// Gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// List of available brokers
    pub brokers: HashMap<String, BrokerConfig>,
    /// Default broker to use
    pub default_broker: String,
    /// WebSocket server configuration
    pub websocket: WebSocketConfig,
    /// REST API configuration
    pub rest_api: RestApiConfig,
    /// Subscription settings
    #[serde(default)]
    pub subscription: SubscriptionConfig,
    /// Log level
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            default_instruments: vec![],
            auto_subscribe_patterns: vec![],
        }
    }
}

impl Config {
    /// Load configuration from a file
    pub fn from_file<P: AsRef<Path>>(path: P) -> GatewayResult<Self> {
        let mut file = File::open(path).map_err(GatewayError::IoError)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(GatewayError::IoError)?;

        let config: Config = serde_json::from_str(&contents).map_err(|e| {
            GatewayError::ConfigError(format!("Failed to parse config file: {}", e))
        })?;
        Ok(config)
    }

    /// Get broker configuration by name
    pub fn get_broker(&self, broker_name: Option<&str>) -> GatewayResult<&BrokerConfig> {
        let broker_name = broker_name.unwrap_or(&self.default_broker);
        self.brokers.get(broker_name).ok_or_else(|| {
            GatewayError::ConfigError(format!("Broker config not found: {}", broker_name))
        })
    }

    /// Get broker configuration by name
    pub fn get_brokers(&self) -> GatewayResult<Vec<BrokerConfig>> {
        let brokers: Vec<BrokerConfig> = self.brokers.values().cloned().collect();
        Ok(brokers)
    }

    /// Load configuration from environment or file
    pub fn load() -> GatewayResult<Self> {
        // Try to read from environment variable first
        if let Ok(config_json) = env::var("QAMDGATEWAY_CONFIG") {
            return serde_json::from_str(&config_json).map_err(|e| {
                GatewayError::ConfigError(format!("Failed to parse config from environment: {}", e))
            });
        }

        // Then try to read from a config file
        let config_path = env::var("QAMDGATEWAY_CONFIG_PATH").unwrap_or_else(|_| {
            // Default paths to try
            for path in ["./config_ctp.json", "./qamdgateway.json"] {
                if Path::new(path).exists() {
                    return path.to_string();
                }
            }
            "./config.json".to_string()
        });

        Self::from_file(config_path)
    }
}
