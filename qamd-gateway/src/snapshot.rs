use crate::types::OptionalF64;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Market data snapshot with order book and trade information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MDSnapshot {
    /// Unique identifier for the instrument (e.g., "SSE_688286")
    pub instrument_id: String,

    /// Total turnover value
    pub amount: f64,

    /// Best ask price (level 1)
    pub ask_price1: f64,
    /// Ask price level 2
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price2: Option<f64>,
    /// Ask price level 3
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price3: Option<f64>,
    /// Ask price level 4
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price4: Option<f64>,
    /// Ask price level 5
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price5: Option<f64>,
    /// Ask price level 6
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price6: Option<f64>,
    /// Ask price level 7
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price7: Option<f64>,
    /// Ask price level 8
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price8: Option<f64>,
    /// Ask price level 9
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price9: Option<f64>,
    /// Ask price level 10
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_price10: Option<f64>,

    /// Best ask volume (level 1)
    pub ask_volume1: i64,
    /// Ask volume level 2
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume2: Option<i64>,
    /// Ask volume level 3
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume3: Option<i64>,
    /// Ask volume level 4
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume4: Option<i64>,
    /// Ask volume level 5
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume5: Option<i64>,
    /// Ask volume level 6
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume6: Option<i64>,
    /// Ask volume level 7
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume7: Option<i64>,
    /// Ask volume level 8
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume8: Option<i64>,
    /// Ask volume level 9
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume9: Option<i64>,
    /// Ask volume level 10
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_volume10: Option<i64>,

    /// Best bid price (level 1)
    pub bid_price1: f64,
    /// Bid price level 2
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price2: Option<f64>,
    /// Bid price level 3
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price3: Option<f64>,
    /// Bid price level 4
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price4: Option<f64>,
    /// Bid price level 5
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price5: Option<f64>,
    /// Bid price level 6
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price6: Option<f64>,
    /// Bid price level 7
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price7: Option<f64>,
    /// Bid price level 8
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price8: Option<f64>,
    /// Bid price level 9
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price9: Option<f64>,
    /// Bid price level 10
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_price10: Option<f64>,

    /// Best bid volume (level 1)
    pub bid_volume1: i64,
    /// Bid volume level 2
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume2: Option<i64>,
    /// Bid volume level 3
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume3: Option<i64>,
    /// Bid volume level 4
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume4: Option<i64>,
    /// Bid volume level 5
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume5: Option<i64>,
    /// Bid volume level 6
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume6: Option<i64>,
    /// Bid volume level 7
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume7: Option<i64>,
    /// Bid volume level 8
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume8: Option<i64>,
    /// Bid volume level 9
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume9: Option<i64>,
    /// Bid volume level 10
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_volume10: Option<i64>,

    /// Closing price for the day, can be "-" before market close
    pub close: OptionalF64,

    /// Timestamp of the snapshot
    pub datetime: DateTime<Utc>,

    /// Highest price of the day
    pub highest: f64,

    /// Last traded price
    pub last_price: f64,

    /// Lower limit price for the day
    pub lower_limit: f64,

    /// Lowest price of the day
    pub lowest: f64,

    /// Opening price for the day
    pub open: f64,

    /// Open interest for futures or options, can be "-" for stocks
    pub open_interest: OptionalF64,

    /// Previous closing price
    pub pre_close: f64,

    /// Previous day's open interest, can be "-" for stocks
    pub pre_open_interest: OptionalF64,

    /// Previous settlement price, can be "-" for stocks
    pub pre_settlement: OptionalF64,

    /// Settlement price, can be "-" for stocks or before market close
    pub settlement: OptionalF64,

    /// Upper limit price for the day
    pub upper_limit: f64,

    /// Total trading volume for the day
    pub volume: i64,

    /// Volume-weighted average price
    pub average: f64,

    /// Indicative Optimized Portfolio Value, used for ETFs, can be "-" for non-ETFs
    pub iopv: OptionalF64,
}

impl MDSnapshot {
    /// Check if the market data includes level 2 depth
    pub fn has_level2_depth(&self) -> bool {
        self.ask_price2.is_some() || self.bid_price2.is_some()
    }

    /// Check if this is futures or options data (has open interest)
    pub fn is_futures_or_options(&self) -> bool {
        matches!(self.open_interest, OptionalF64::Value(_))
    }

    /// Check if this is an ETF (has IOPV)
    pub fn is_etf(&self) -> bool {
        matches!(self.iopv, OptionalF64::Value(_))
    }

    /// Calculate bid-ask spread
    pub fn bid_ask_spread(&self) -> f64 {
        self.ask_price1 - self.bid_price1
    }
}
