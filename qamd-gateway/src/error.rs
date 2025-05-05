use thiserror::Error;

/// Errors that can occur in the QAMD library
#[derive(Error, Debug)]
pub enum MDError {
    /// Error during serialization or deserialization
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Error parsing a date or time
    #[error("Date/time parse error: {0}")]
    DateTimeParseError(#[from] chrono::ParseError),

    /// Market data is invalid or missing required fields
    #[error("Invalid market data: {0}")]
    InvalidMarketData(String),

    /// General error
    #[error("{0}")]
    General(String),
}

/// Custom error types for the QAMD Gateway
#[derive(Error, Debug)]
pub enum GatewayError {
    /// IO errors
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// JSON serialization/deserialization errors
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// CTP errors
    #[error("CTP error: {0}")]
    CtpError(String),

    /// QAMD errors
    #[error("QAMD error: {0}")]
    QamdError(#[from] MDError),

    /// Market data conversion errors
    #[error("Market data conversion error: {0}")]
    ConversionError(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// WebSocket errors
    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    /// Invalid instrument error
    #[error("Invalid instrument: {0}")]
    InvalidInstrument(String),

    /// Authentication error
    #[error("Authentication error: {0}")]
    AuthError(String),

    /// Other errors
    #[error("Other error: {0}")]
    Other(String),
}

/// Result type for the QAMD Gateway
pub type GatewayResult<T> = Result<T, GatewayError>;
