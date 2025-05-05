use actix::prelude::*;
use ctp2rs::v1alpha1::CThostFtdcDepthMarketDataField;
use std::collections::HashMap;

use crate::actors::md_distributor::MarketDataDistributor;
use crate::snapshot::MDSnapshot;
// // Message type forward declarations for feature-dependent types
// #[cfg(feature = "qq")]
// pub use crate::actors::md_actor as qq_md_actor;
// #[cfg(feature = "sina")]
// pub use crate::actors::md_actor as sina_md_actor;

/// 市场数据源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarketDataSource {
    CTP,
    QQ,
    Sina,
}

//
// 通用市场数据Actor消息
//

/// 初始化市场数据源
#[derive(Message)]
#[rtype(result = "()")]
pub struct InitMarketDataSource;

/// 登录市场数据源
#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct LoginMarketDataSource;

/// 启动市场数据流
#[derive(Message)]
#[rtype(result = "()")]
pub struct StartMarketData {
    pub instruments: Vec<String>,
}

/// 停止市场数据流
#[derive(Message)]
#[rtype(result = "()")]
pub struct StopMarketData;

/// 订阅市场数据
#[derive(Message)]
#[rtype(result = "()")]
pub struct Subscribe {
    /// 客户端ID
    pub id: uuid::Uuid,
    /// 要订阅的合约列表
    pub instruments: Vec<String>,
}

/// 取消订阅市场数据
#[derive(Message)]
#[rtype(result = "()")]
pub struct Unsubscribe {
    /// 客户端ID
    pub id: uuid::Uuid,
    /// 要取消订阅的合约列表
    pub instruments: Vec<String>,
}

/// 获取当前订阅的合约列表
#[derive(Message)]
#[rtype(result = "Vec<String>")]
pub struct GetSubscriptions {
    /// 客户端ID
    pub id: uuid::Uuid,
    /// 订阅回调（可选）
    pub callback: Option<Box<dyn Fn(Vec<String>) + Send>>,
}

/// 注册市场数据分发器
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterDistributor {
    pub addr: Addr<MarketDataDistributor>,
}

/// 市场数据事件（内部使用）
#[derive(Message)]
#[rtype(result = "()")]
pub enum MarketDataEvent {
    Connected,
    Disconnected,
    LoggedIn,
    MarketData(CThostFtdcDepthMarketDataField),
    SubscriptionSuccess(String),
    SubscriptionFailure(String, String),
    Error(String),
}

/// 重启 Actor
#[derive(Message)]
#[rtype(result = "()")]
pub struct RestartActor;

//
// WebSocket 服务器消息
//

/// WebSocket 客户端连接消息
#[derive(Message)]
#[rtype(result = "()")]
pub struct Connect {
    pub addr: Recipient<WSMessage>,
    pub client_id: String,
}

/// WebSocket 客户端断开连接消息
#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    pub client_id: String,
}

/// WebSocket 消息
#[derive(Message)]
#[rtype(result = "()")]
pub struct WSMessage(pub String);

/// 全局注册信息消息
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterInfo {
    pub message: String,
}

//
// 市场数据分发器消息
//

/// 注册市场数据接收者
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterDataReceiver {
    pub client_id: String,
    pub addr: Recipient<MarketDataUpdateMessage>,
    pub instruments: Vec<String>,
}

/// 取消注册市场数据接收者
#[derive(Message)]
#[rtype(result = "()")]
pub struct UnregisterDataReceiver {
    pub client_id: String,
}

/// 更新市场数据订阅
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateSubscription {
    pub client_id: String,
    pub instruments: Vec<String>,
}

/// 查询当前订阅
#[derive(Message)]
#[rtype(result = "Vec<String>")]
pub struct QuerySubscription {
    pub client_id: String,
}

/// 市场数据更新消息传递给客户端
#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct MarketDataUpdateMessage {
    pub instruments: Vec<String>,
    pub data: HashMap<String, String>, // 使用JSON字符串表示行情数据
}

/// 市场数据更新消息传递给分发器
#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct MarketDataUpdate(pub MDSnapshot, pub MarketDataSource);

/// 获取所有订阅列表消息
#[derive(Message)]
#[rtype(result = "Vec<String>")]
pub struct GetAllSubscriptions {}

//
// 针对特定市场数据源的注册消息
//

/// 注册CTP市场数据Actor
// #[cfg(feature = "ctp")]
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterCTPMdActor {
    pub broker_id: String,
    pub addr: Addr<crate::actors::md_actor::MarketDataActor>,
}

// /// 注册QQ市场数据Actor
// #[cfg(feature = "qq")]
// #[derive(Message)]
// #[rtype(result = "()")]
// pub struct RegisterQQMdActor {
//     pub broker_id: String,
//     pub addr: Addr<crate::actors::md_actor::MarketDataActor>,
// }

// /// 注册新浪市场数据Actor
// #[cfg(feature = "sina")]
// #[derive(Message)]
// #[rtype(result = "()")]
// pub struct RegisterSinaMdActor {
//     pub broker_id: String,
//     pub addr: Addr<crate::actors::md_actor::MarketDataActor>,
// }

// 创建市场数据Actor时的注册消息
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterMdActor {
    pub broker_id: String,
    pub addr: Addr<crate::actors::md_actor::MarketDataActor>,
    pub source_type: MarketDataSource,
}

/// 添加单个订阅消息
#[derive(Message)]
#[rtype(result = "()")]
pub struct AddSubscription {
    /// 要订阅的合约
    pub instrument: String,
    /// 客户端ID
    pub client_id: uuid::Uuid,
}

/// 移除单个订阅消息
#[derive(Message)]
#[rtype(result = "()")]
pub struct RemoveSubscription {
    /// 要取消订阅的合约
    pub instrument: String,
    /// 客户端ID
    pub client_id: uuid::Uuid,
}

/// WebSocket连接消息
#[derive(Message)]
#[rtype(result = "()")]
pub struct WebSocketConnect {
    /// 客户端ID
    pub id: uuid::Uuid,
    /// 客户端地址
    pub addr: Recipient<MarketDataUpdateMessage>,
}

/// WebSocket断开消息
#[derive(Message)]
#[rtype(result = "()")]
pub struct WebSocketDisconnect {
    /// 客户端ID
    pub id: uuid::Uuid,
}

#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct WebSocketMessage(pub String);
