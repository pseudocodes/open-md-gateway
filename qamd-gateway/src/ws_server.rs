use actix::{Actor, ActorContext, AsyncContext, Handler, StreamHandler};
use actix_web::http::StatusCode;
use actix_web::{web, Error, HttpRequest, HttpResponse, Responder};
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use log::{debug, error, info, warn};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::actors::md_distributor::MarketDataDistributor;
use crate::actors::messages::*;

use crate::snapshot::MDSnapshot;

// 心跳间隔，保持连接活跃（10秒）
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
// 如果客户端在此期间未响应ping，则终止连接（30秒）
const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);

/// WebSocket客户端消息类型
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WsClientMessage {
    /// TradingView格式订阅行情
    #[serde(rename_all = "snake_case")]
    TvSubscribeQuote { aid: String, ins_list: String },
    /// 传统格式兼容
    LegacyMessage(LegacyClientMessage),
    /// Peek message
    #[serde(rename_all = "snake_case")]
    PeekMessage { aid: String },
}

/// 兼容旧版本的消息格式
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum LegacyClientMessage {
    /// 订阅一个或多个合约
    #[serde(rename = "subscribe")]
    Subscribe { instruments: Vec<String> },
    /// 取消订阅一个或多个合约
    #[serde(rename = "unsubscribe")]
    Unsubscribe { instruments: Vec<String> },
    /// 获取所有已订阅的合约
    #[serde(rename = "subscriptions")]
    Subscriptions,
    /// 认证（如果需要）
    #[serde(rename = "auth")]
    Auth { token: String },
    /// 保持连接的ping消息
    #[serde(rename = "ping")]
    Ping,
}

/// WebSocket服务器消息类型
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WsServerMessage {
    /// TradingView格式的行情数据
    TvMarketData {
        aid: String,
        data: Vec<TvMarketDataItem>,
    },
    /// 旧版格式
    LegacyMessage(LegacyServerMessage),
    /// Peek message响应
    PeekMessageResponse { aid: String, ins_list: String },
}

/// TradingView格式的行情数据项
#[derive(Debug, Serialize, Deserialize)]
pub struct TvMarketDataItem {
    pub quotes: HashMap<String, TvQuote>,
}

/// TradingView格式的行情数据
#[derive(Debug, Serialize, Deserialize)]
pub struct TvQuote {
    pub instrument_id: String,
    pub datetime: String,
    pub last_price: f64,
    pub volume: i64,
    pub amount: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub bid_price1: f64,
    pub bid_volume1: i64,
    pub ask_price1: f64,
    pub ask_volume1: i64,
    pub volume_multiple: i32,
    pub price_tick: f64,
    #[serde(default)]
    pub price_decs: i32,
    #[serde(default)]
    pub max_market_order_volume: i64,
    #[serde(default)]
    pub min_market_order_volume: i64,
    #[serde(default)]
    pub max_limit_order_volume: i64,
    #[serde(default)]
    pub min_limit_order_volume: i64,
    #[serde(default)]
    pub margin: f64,
    #[serde(default)]
    pub commission: f64,
    #[serde(default)]
    pub upper_limit: f64,
    #[serde(default)]
    pub lower_limit: f64,
    #[serde(default)]
    pub pre_close: f64,
    #[serde(default)]
    pub pre_settlement: f64,
    #[serde(default)]
    pub pre_open_interest: i64,
    #[serde(default)]
    pub open_interest: i64,
    #[serde(default)]
    pub close: f64,
    #[serde(default)]
    pub settlement: f64,
    #[serde(default)]
    pub average: f64,
}

/// 传统服务器消息格式
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum LegacyServerMessage {
    /// 市场数据更新
    #[serde(rename = "market_data")]
    MarketData { data: MDSnapshot },
    /// 系统消息
    #[serde(rename = "system")]
    System { message: String },
    /// 错误消息
    #[serde(rename = "error")]
    Error { message: String },
    /// 订阅请求的响应
    #[serde(rename = "subscriptions")]
    Subscriptions { instruments: Vec<String> },
    /// 对ping的pong响应
    #[serde(rename = "pong")]
    Pong,
}

/// WebSocket会话状态
pub struct WsSession {
    /// 唯一会话ID
    client_id: String,
    /// 客户端心跳状态
    heartbeat: Instant,
    /// 市场数据分发器地址
    md_distributor: actix::Addr<MarketDataDistributor>,
    /// 已订阅的合约
    subscriptions: HashSet<String>,
    // / 市场数据源类型
    market_data_source: MarketDataSource,
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // 启动心跳进程
        self.start_heartbeat(ctx);

        // 注册到市场数据分发器
        let addr = ctx.address();

        // 向分发器注册，提供会话ID和接收者地址
        self.md_distributor.do_send(RegisterDataReceiver {
            client_id: self.client_id.clone(),
            addr: addr.recipient(),
            instruments: Vec::new(),
        });

        // 发送欢迎消息
        let msg = WsServerMessage::LegacyMessage(LegacyServerMessage::System {
            message: format!(
                "Connected to QAMD Gateway WebSocket. Session ID: {}",
                self.client_id
            ),
        });
        if let Ok(json) = serde_json::to_string(&msg) {
            ctx.text(json);
        }
    }

    fn stopping(&mut self, _: &mut Self::Context) -> actix::Running {
        // 从市场数据分发器取消注册
        self.md_distributor.do_send(UnregisterDataReceiver {
            client_id: self.client_id.clone(),
        });
        actix::Running::Stop
    }
}

impl WsSession {
    /// 创建新的WebSocket会话
    pub fn new(
        md_distributor: actix::Addr<MarketDataDistributor>,
        source: MarketDataSource,
    ) -> Self {
        Self {
            client_id: Uuid::new_v4().to_string(),
            heartbeat: Instant::now(),
            md_distributor,
            subscriptions: HashSet::new(),
            market_data_source: source,
        }
    }

    /// 启动心跳检测
    fn start_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // 检查客户端心跳
            if Instant::now().duration_since(act.heartbeat) > CLIENT_TIMEOUT {
                // 心跳超时，关闭连接
                info!(
                    "WebSocket Client {} heartbeat failed, disconnecting",
                    act.client_id
                );
                ctx.stop();
                return;
            }

            // 发送ping
            ctx.ping(b"");
        });
    }

    /// 将TradingView格式的订阅字符串转换为合约列表
    fn parse_tv_instruments(&self, ins_list: &str) -> Vec<String> {
        ins_list
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    /// 处理订阅请求
    fn handle_subscribe(&mut self, ctx: &mut ws::WebsocketContext<Self>, instruments: Vec<String>) {
        if instruments.is_empty() {
            let msg = WsServerMessage::LegacyMessage(LegacyServerMessage::Error {
                message: "No instruments specified".to_string(),
            });
            if let Ok(json) = serde_json::to_string(&msg) {
                ctx.text(json);
            }
            return;
        }

        // 更新本地订阅集合
        for instrument in &instruments {
            self.subscriptions.insert(instrument.clone());
        }

        // 更新分发器的订阅
        self.md_distributor.do_send(UpdateSubscription {
            client_id: self.client_id.clone(),
            instruments: instruments.clone(),
        });

        // 发送确认消息
        let msg: WsServerMessage = WsServerMessage::LegacyMessage(LegacyServerMessage::System {
            message: format!("Subscribed to {} instruments", instruments.len()),
        });
        if let Ok(json) = serde_json::to_string(&msg) {
            ctx.text(json);
        }
    }

    /// 处理取消订阅请求
    fn handle_unsubscribe(
        &mut self,
        ctx: &mut ws::WebsocketContext<Self>,
        instruments: Vec<String>,
    ) {
        if instruments.is_empty() {
            let msg = WsServerMessage::LegacyMessage(LegacyServerMessage::Error {
                message: "No instruments specified".to_string(),
            });
            if let Ok(json) = serde_json::to_string(&msg) {
                ctx.text(json);
            }
            return;
        }

        // 更新本地订阅集合
        for instrument in &instruments {
            self.subscriptions.remove(instrument);
        }

        // 获取当前所有订阅
        let current_subscriptions: Vec<String> = self.subscriptions.iter().cloned().collect();

        // 更新分发器的订阅
        self.md_distributor.do_send(UpdateSubscription {
            client_id: self.client_id.clone(),
            instruments: current_subscriptions,
        });

        // 发送确认消息
        let msg = WsServerMessage::LegacyMessage(LegacyServerMessage::System {
            message: format!("Unsubscribed from {} instruments", instruments.len()),
        });
        if let Ok(json) = serde_json::to_string(&msg) {
            ctx.text(json);
        }
    }

    /// 处理获取订阅列表请求
    fn handle_get_subscriptions(&self, ctx: &mut ws::WebsocketContext<Self>) {
        // 发送当前订阅列表
        let subscriptions: Vec<String> = self.subscriptions.iter().cloned().collect();
        let msg = WsServerMessage::LegacyMessage(LegacyServerMessage::Subscriptions {
            instruments: subscriptions,
        });
        if let Ok(json) = serde_json::to_string(&msg) {
            ctx.text(json);
        }
    }
}

/// 处理来自WebSocket的消息
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.heartbeat = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.heartbeat = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                self.heartbeat = Instant::now();

                // 尝试解析消息
                match serde_json::from_str::<WsClientMessage>(&text) {
                    Ok(WsClientMessage::TvSubscribeQuote { aid, ins_list })
                        if aid == "subscribe_quote" =>
                    {
                        // TradingView格式的订阅
                        let instruments = self.parse_tv_instruments(&ins_list);
                        self.handle_subscribe(ctx, instruments);

                        // 发送订阅确认，返回订阅列表
                        let msg = WsServerMessage::PeekMessageResponse {
                            aid: "rsp_subscribe_quote".to_string(),
                            ins_list,
                        };
                        if let Ok(json) = serde_json::to_string(&msg) {
                            ctx.text(json);
                        }
                    }
                    Ok(WsClientMessage::PeekMessage { aid }) if aid == "peek_message" => {
                        // 查询当前订阅列表并返回TradingView格式
                        let subscriptions: Vec<String> =
                            self.subscriptions.iter().cloned().collect();
                        let ins_list = subscriptions.join(",");

                        let msg = WsServerMessage::PeekMessageResponse {
                            aid: "rsp_peek_message".to_string(),
                            ins_list,
                        };
                        if let Ok(json) = serde_json::to_string(&msg) {
                            ctx.text(json);
                        }
                    }
                    Ok(WsClientMessage::LegacyMessage(client_msg)) => {
                        match client_msg {
                            LegacyClientMessage::Subscribe { instruments } => {
                                // 处理传统格式的订阅
                                self.handle_subscribe(ctx, instruments);
                            }
                            LegacyClientMessage::Unsubscribe { instruments } => {
                                // 处理传统格式的取消订阅
                                self.handle_unsubscribe(ctx, instruments);
                            }
                            LegacyClientMessage::Subscriptions => {
                                // 处理获取订阅列表请求
                                self.handle_get_subscriptions(ctx);
                            }
                            LegacyClientMessage::Auth { token: _ } => {
                                // 目前不处理认证
                                let msg =
                                    WsServerMessage::LegacyMessage(LegacyServerMessage::System {
                                        message: "Authentication not implemented".to_string(),
                                    });
                                if let Ok(json) = serde_json::to_string(&msg) {
                                    ctx.text(json);
                                }
                            }
                            LegacyClientMessage::Ping => {
                                // 响应ping
                                let msg = WsServerMessage::LegacyMessage(LegacyServerMessage::Pong);
                                if let Ok(json) = serde_json::to_string(&msg) {
                                    ctx.text(json);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // 消息解析错误
                        error!("Failed to parse WebSocket message: {}", e);
                        let msg = WsServerMessage::LegacyMessage(LegacyServerMessage::Error {
                            message: format!("Invalid message format: {}", e),
                        });
                        if let Ok(json) = serde_json::to_string(&msg) {
                            ctx.text(json);
                        }
                    }
                    _ => {
                        // 未知消息类型
                        warn!("Unknown WebSocket message type: {}", text);
                        let msg = WsServerMessage::LegacyMessage(LegacyServerMessage::Error {
                            message: "Unknown message type".to_string(),
                        });
                        if let Ok(json) = serde_json::to_string(&msg) {
                            ctx.text(json);
                        }
                    }
                }
            }
            Ok(ws::Message::Binary(_)) => {
                warn!("Binary WebSocket messages are not supported");
            }
            Ok(ws::Message::Close(reason)) => {
                info!("WebSocket connection closed: {:?}", reason);
                ctx.close(reason);
                ctx.stop();
            }
            _ => {
                ctx.stop();
            }
        }
    }
}

/// 处理从分发器接收到的市场数据更新
impl Handler<MarketDataUpdateMessage> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: MarketDataUpdateMessage, ctx: &mut Self::Context) {
        // 遍历收到的合约数据
        for instrument in &msg.instruments {
            // 检查该客户端是否订阅了该合约
            if self.subscriptions.contains(instrument) {
                if let Some(data_json) = msg.data.get(instrument) {
                    // 将JSON字符串解析为Value对象
                    if let Ok(data_value) = serde_json::from_str::<Value>(data_json) {
                        // 创建TradingView格式的响应
                        let mut quotes = HashMap::new();

                        // 从data_value提取字段创建TvQuote
                        // 注意：这里的数据可能是增量的，只包含变化的字段
                        if let Some(instrument_id) =
                            data_value.get("instrument_id").and_then(|v| v.as_str())
                        {
                            // 只处理拥有instrument_id字段的数据
                            quotes.insert(instrument_id.to_string(), data_value.clone());

                            // 创建TradingView格式的市场数据响应
                            let tv_market_data = json!({
                                "aid": "rtn_data",
                                "data": [
                                    {
                                        "quotes": quotes
                                    }
                                ]
                            });

                            // 将响应发送给客户端
                            if let Ok(json_str) = serde_json::to_string(&tv_market_data) {
                                ctx.text(json_str);
                                debug!(
                                    "Sent market data update for {} to client {}",
                                    instrument, self.client_id
                                );
                            } else {
                                error!("Failed to serialize market data for {}", instrument);
                            }
                        } else {
                            error!("Market data missing instrument_id field: {}", data_json);
                        }
                    } else {
                        error!(
                            "Failed to parse market data JSON for {}: {}",
                            instrument, data_json
                        );
                    }
                }
            }
        }
    }
}

/// 创建WebSocket处理器
pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    md_distributor: web::Data<actix::Addr<MarketDataDistributor>>,
) -> Result<HttpResponse, Error> {
    // 获取查询参数
    // let query = req.query_string();
    info!("match_info: {:?}", req.match_info().get("source"));
    let source_type = match req.match_info().get("source") {
        Some("ctp") => MarketDataSource::CTP,
        Some("qq") => MarketDataSource::QQ,
        Some("sina") => MarketDataSource::Sina,
        Some(&_) => MarketDataSource::CTP,
        None => MarketDataSource::CTP,
    };

    // 创建WebSocket会话
    let session = WsSession::new(md_distributor.get_ref().clone(), source_type);

    // 启动WebSocket连接
    let resp = ws::start(session, &req, stream)?;
    Ok(resp)
}
