use chrono::{DateTime, Local, TimeZone, Utc};

use std::str::FromStr;

use ctp2rs::{ffi::WrapToString, v1alpha1::CThostFtdcDepthMarketDataField};

use crate::error::{GatewayError, GatewayResult};

use crate::snapshot::MDSnapshot;
use crate::types::OptionalF64;

/// Converts CTP market data to QAMD MDSnapshot
pub fn convert_ctp_to_md_snapshot(
    ctp_data: &CThostFtdcDepthMarketDataField,
) -> GatewayResult<MDSnapshot> {
    // Parse the update time from CTP format
    let datetime = parse_ctp_datetime(
        &ctp_data.TradingDay.to_string(),
        &ctp_data.UpdateTime.to_string(),
        ctp_data.UpdateMillisec,
    )?;

    // Extract exchange ID and instrument ID
    let instrument_id = format_instrument_id(
        &ctp_data.ExchangeID.to_string(),
        &ctp_data.InstrumentID.to_string(),
    )?;

    // Helper function to safely convert CTP numeric strings to f64
    let _parse_f64 = |s: &[u8]| -> Result<f64, GatewayError> {
        let s = std::str::from_utf8(s)
            .map_err(|_| GatewayError::ConversionError("Invalid UTF-8 string".to_string()))?
            .trim_end_matches('\0')
            .trim();

        if s.is_empty() || s == "-" {
            return Ok(0.0);
        }

        f64::from_str(s).map_err(|_| {
            GatewayError::ConversionError(format!("Failed to parse f64 from string: {}", s))
        })
    };

    // Helper function to convert CTP optional prices to Option<f64>
    let optional_price = |price: f64| -> Option<f64> {
        if price <= 0.0 || price.is_nan() || price == f64::MAX {
            None
        } else {
            Some(price)
        }
    };

    // Helper function to convert CTP optional volumes to Option<i64>
    let optional_volume = |volume: i32| -> Option<i64> {
        if volume <= 0 {
            None
        } else {
            Some(volume as i64)
        }
    };

    // Helper function to create OptionalF64 for special fields
    let optional_f64 = |value: f64, field_name: &str| -> OptionalF64 {
        if value <= 0.0 || value.is_nan() {
            // For futures-specific fields, use "-" when not available
            if field_name == "open_interest"
                || field_name == "pre_open_interest"
                || field_name == "settlement"
                || field_name == "pre_settlement"
            {
                OptionalF64::String("-".to_string())
            } else {
                OptionalF64::Null
            }
        } else {
            OptionalF64::Value(value)
        }
    };

    // Create MDSnapshot from CTP data
    let snapshot = MDSnapshot {
        instrument_id,
        amount: ctp_data.Turnover as f64,
        ask_price1: ctp_data.AskPrice1,
        ask_volume1: ctp_data.AskVolume1 as i64,
        bid_price1: ctp_data.BidPrice1,
        bid_volume1: ctp_data.BidVolume1 as i64,
        last_price: ctp_data.LastPrice,
        datetime,
        highest: ctp_data.HighestPrice,
        lowest: ctp_data.LowestPrice,
        open: ctp_data.OpenPrice,
        close: optional_f64(ctp_data.ClosePrice, "close"),
        volume: ctp_data.Volume as i64,
        pre_close: ctp_data.PreClosePrice,
        lower_limit: ctp_data.LowerLimitPrice,
        upper_limit: ctp_data.UpperLimitPrice,
        average: ctp_data.AveragePrice,

        // Optional depth levels
        ask_price2: optional_price(ctp_data.AskPrice2),
        ask_volume2: optional_volume(ctp_data.AskVolume2),
        bid_price2: optional_price(ctp_data.BidPrice2),
        bid_volume2: optional_volume(ctp_data.BidVolume2),
        ask_price3: optional_price(ctp_data.AskPrice3),
        ask_volume3: optional_volume(ctp_data.AskVolume3),
        bid_price3: optional_price(ctp_data.BidPrice3),
        bid_volume3: optional_volume(ctp_data.BidVolume3),
        ask_price4: optional_price(ctp_data.AskPrice4),
        ask_volume4: optional_volume(ctp_data.AskVolume4),
        bid_price4: optional_price(ctp_data.BidPrice4),
        bid_volume4: optional_volume(ctp_data.BidVolume4),
        ask_price5: optional_price(ctp_data.AskPrice5),
        ask_volume5: optional_volume(ctp_data.AskVolume5),
        bid_price5: optional_price(ctp_data.BidPrice5),
        bid_volume5: optional_volume(ctp_data.BidVolume5),

        // Additional optional depth levels (not available in CTP, set to None)
        ask_price6: None,
        ask_volume6: None,
        bid_price6: None,
        bid_volume6: None,
        ask_price7: None,
        ask_volume7: None,
        bid_price7: None,
        bid_volume7: None,
        ask_price8: None,
        ask_volume8: None,
        bid_price8: None,
        bid_volume8: None,
        ask_price9: None,
        ask_volume9: None,
        bid_price9: None,
        bid_volume9: None,
        ask_price10: None,
        ask_volume10: None,
        bid_price10: None,
        bid_volume10: None,

        // Futures-specific fields
        open_interest: optional_f64(ctp_data.OpenInterest, "open_interest"),
        pre_open_interest: optional_f64(ctp_data.PreOpenInterest, "pre_open_interest"),
        settlement: optional_f64(ctp_data.SettlementPrice, "settlement"),
        pre_settlement: optional_f64(ctp_data.PreSettlementPrice, "pre_settlement"),

        // ETF-specific fields (not available in CTP, set to Null)
        iopv: OptionalF64::Null,
    };

    Ok(snapshot)
}

/// Parse CTP datetime format (trading_day + update_time + millisec) into a UTC DateTime
fn parse_ctp_datetime(
    trading_day: &str,
    update_time: &str,
    millisec: i32,
) -> GatewayResult<DateTime<Utc>> {
    // Convert from byte arrays to strings

    // Parse the date and time components
    let year = i32::from_str(&trading_day[0..4]).map_err(|_| {
        GatewayError::ConversionError(format!("Invalid year in trading day: {}", trading_day))
    })?;
    let month = u32::from_str(&trading_day[4..6]).map_err(|_| {
        GatewayError::ConversionError(format!("Invalid month in trading day: {}", trading_day))
    })?;
    let day = u32::from_str(&trading_day[6..8]).map_err(|_| {
        GatewayError::ConversionError(format!("Invalid day in trading day: {}", trading_day))
    })?;

    // Handle night trading session (next day's trading date)
    // If update_time is after 17:00, we're in night trading for the next day
    let (hour, minute, second) = if update_time.len() >= 8 {
        let h = u32::from_str(&update_time[0..2]).map_err(|_| {
            GatewayError::ConversionError(format!("Invalid hour in update time: {}", update_time))
        })?;
        let m = u32::from_str(&update_time[3..5]).map_err(|_| {
            GatewayError::ConversionError(format!("Invalid minute in update time: {}", update_time))
        })?;
        let s = u32::from_str(&update_time[6..8]).map_err(|_| {
            GatewayError::ConversionError(format!("Invalid second in update time: {}", update_time))
        })?;
        (h, m, s)
    } else {
        (0, 0, 0)
    };

    // Create a naive datetime
    let naive_dt = match chrono::NaiveDateTime::new(
        chrono::NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| {
            GatewayError::ConversionError(format!("Invalid date: {}/{}/{}", year, month, day))
        })?,
        chrono::NaiveTime::from_hms_milli_opt(hour, minute, second, millisec as u32).ok_or_else(
            || {
                GatewayError::ConversionError(format!(
                    "Invalid time: {}:{}:{}.{}",
                    hour, minute, second, millisec
                ))
            },
        )?,
    ) {
        dt => dt,
    };

    // Convert to UTC
    Ok(Local
        .from_local_datetime(&naive_dt)
        .unwrap()
        .with_timezone(&Utc))
}

/// Format instrument ID with exchange prefix
fn format_instrument_id(exchange_id: &str, instrument_id: &str) -> GatewayResult<String> {
    // Map CTP exchange IDs to QAMD exchange format
    let exchange_prefix = match exchange_id {
        "SHFE" => "SHFE.",     // Shanghai Futures Exchange
        "DCE" => "DCE.",       // Dalian Commodity Exchange
        "CZCE" => "CZCE.",     // Zhengzhou Commodity Exchange
        "CFFEX" => "CFFEX.",   // China Financial Futures Exchange
        "INE" => "INE.",       // Shanghai International Energy Exchange
        "GFEX" => "GFEX.",     // GuangZhou Commodity Exchange
        "SSE" => "SSE.",       // Shanghai Stock Exchange
        "SZSE" => "SZSE.",     // Shenzhen Stock Exchange
        "HKEX" => "HKEX.",     // Hong Kong Stock Exchange
        "NYSE" => "NYSE.",     // New York Stock Exchange
        "NASDAQ" => "NASDAQ.", // Nasdaq Stock Market
        "AMEX" => "AMEX.",     // American Stock Exchange
        "BSE" => "BSE.",       // Bombay Stock Exchange
        "NSE" => "NSE.",       // National Stock Exchange of India
        _ => "",               // No prefix for unknown exchanges
    };

    Ok(format!("{}{}", exchange_prefix, instrument_id))
}
