use actix::prelude::*;
use log::{debug, error, info, warn};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use uuid;

use crate::actors::messages::*;
use crate::snapshot::MDSnapshot;
use crate::types::OptionalF64;

/// 市场数据分发器
///
/// 负责接收来自不同市场数据源的行情数据，
/// 并根据客户端订阅将数据转发给对应的接收者
pub struct MarketDataDistributor {
    // 保存客户端及其订阅关系
    subscribers: HashMap<String, Subscriber>,

    // 保存合约订阅关系 (合约ID -> 订阅客户端集合)
    instrument_subscribers: HashMap<String, HashSet<String>>,

    // 保存不同市场数据源的Actor地址
    // #[cfg(feature = "ctp")]
    ctp_actors: HashMap<String, Addr<crate::actors::md_actor::MarketDataActor>>,

    // #[cfg(feature = "qq")]
    // qq_actors: HashMap<String, Addr<crate::actors::md_actor::MarketDataActor>>,

    // #[cfg(feature = "sina")]
    // sina_actors: HashMap<String, Addr<crate::actors::md_actor::MarketDataActor>>,

    // 最新的市场数据缓存 (合约ID -> 行情数据)
    market_data_cache: HashMap<String, MDSnapshot>,

    // 来源标记 (合约ID -> 市场数据源)
    source_map: HashMap<String, MarketDataSource>,

    // 增量更新相关字段
    // 每个客户端最后的行情数据快照
    client_snapshots: HashMap<String, HashMap<String, MDSnapshot>>,

    // 批量更新累积缓存
    batch_updates: HashMap<String, HashMap<String, serde_json::Value>>,

    // 上次批量发送时间
    last_batch_send: Instant,

    // 批量更新配置
    batch_interval: Duration,
    batch_size_threshold: usize,
}

/// 订阅者信息
struct Subscriber {
    // 客户端地址
    addr: Recipient<MarketDataUpdateMessage>,
    // 订阅的合约集合
    instruments: HashSet<String>,
}

impl Actor for MarketDataDistributor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("MarketDataDistributor started");

        // 添加定时任务，处理累积的批量更新
        ctx.run_interval(self.batch_interval, |act, _| {
            if !act.batch_updates.is_empty() {
                act.send_batch_updates();
            }
        });
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        info!("MarketDataDistributor stopped");
    }
}

impl Default for MarketDataDistributor {
    fn default() -> Self {
        Self::new()
    }
}

impl MarketDataDistributor {
    /// 创建一个新的市场数据分发器
    pub fn new() -> Self {
        Self {
            subscribers: HashMap::new(),
            instrument_subscribers: HashMap::new(),
            // #[cfg(feature = "ctp")]
            ctp_actors: HashMap::new(),
            // #[cfg(feature = "qq")]
            // qq_actors: HashMap::new(),
            // #[cfg(feature = "sina")]
            // sina_actors: HashMap::new(),
            market_data_cache: HashMap::new(),
            source_map: HashMap::new(),
            client_snapshots: HashMap::new(),
            batch_updates: HashMap::new(),
            last_batch_send: Instant::now(),
            batch_interval: Duration::from_millis(100),
            batch_size_threshold: 50,
        }
    }

    /// 添加订阅
    fn add_subscription(&mut self, client_id: &str, instruments: &[String]) {
        // 检查是否为新客户端
        let is_new_client = !self.client_snapshots.contains_key(client_id);

        // Collect instruments with cached data for later use
        let mut instruments_with_data = Vec::new();

        if let Some(subscriber) = self.subscribers.get_mut(client_id) {
            // 更新现有订阅者的订阅
            for instrument in instruments {
                // 检查是否是新订阅的合约
                let is_new_subscription = !subscriber.instruments.contains(instrument);

                subscriber.instruments.insert(instrument.clone());

                // 更新合约订阅关系
                self.instrument_subscribers
                    .entry(instrument.clone())
                    .or_insert_with(HashSet::new)
                    .insert(client_id.to_string());

                // 如果是新订阅的合约，需要发送全量数据
                if is_new_subscription {
                    if let Some(data) = self.market_data_cache.get(instrument) {
                        instruments_with_data.push((instrument.clone(), data.clone()));
                    }
                }
            }
        }

        // 为新订阅的合约发送全量数据
        if !instruments_with_data.is_empty() {
            if let Some(subscriber) = self.subscribers.get(client_id) {
                let mut data_map = HashMap::new();
                let mut update_instruments = Vec::new();

                // 构建全量数据
                for (instrument, data) in &instruments_with_data {
                    let json_data = self.snapshot_to_json(data);
                    data_map.insert(instrument.clone(), json_data.to_string());
                    update_instruments.push(instrument.clone());

                    // 更新客户端快照
                    self.client_snapshots
                        .entry(client_id.to_string())
                        .or_insert_with(HashMap::new)
                        .insert(instrument.clone(), data.clone());
                }

                // 发送全量数据
                let message = MarketDataUpdateMessage {
                    instruments: update_instruments,
                    data: data_map,
                };

                if let Err(e) = subscriber.addr.try_send(message) {
                    error!(
                        "Failed to send full snapshot to client {}: {}",
                        client_id, e
                    );
                } else {
                    debug!(
                        "Sent full snapshot to client {} for {} instruments",
                        client_id,
                        instruments_with_data.len()
                    );
                }
            }
        }
    }

    /// 删除订阅
    fn remove_subscription(&mut self, client_id: &str, instruments: &[String]) {
        if let Some(subscriber) = self.subscribers.get_mut(client_id) {
            // 从订阅者中移除订阅
            for instrument in instruments {
                subscriber.instruments.remove(instrument);

                // 更新合约订阅关系
                if let Some(subscribers) = self.instrument_subscribers.get_mut(instrument) {
                    subscribers.remove(client_id);

                    // 如果没有订阅者了，则考虑取消订阅该合约（从行情源）
                    if subscribers.is_empty() {
                        self.instrument_subscribers.remove(instrument);

                        // 根据数据来源取消订阅合约
                        if let Some(source) = self.source_map.get(instrument) {
                            match source {
                                #[cfg(feature = "ctp")]
                                MarketDataSource::CTP => {
                                    for (_, actor) in &self.ctp_actors {
                                        actor.do_send(Unsubscribe {
                                            id: uuid::Uuid::nil(),
                                            instruments: vec![instrument.clone()],
                                        });
                                    }
                                }
                                #[cfg(feature = "qq")]
                                MarketDataSource::QQ => {
                                    for (_, actor) in &self.qq_actors {
                                        actor.do_send(Unsubscribe {
                                            id: uuid::Uuid::nil(),
                                            instruments: vec![instrument.clone()],
                                        });
                                    }
                                }
                                #[cfg(feature = "sina")]
                                MarketDataSource::Sina => {
                                    for (_, actor) in &self.sina_actors {
                                        actor.do_send(Unsubscribe {
                                            id: uuid::Uuid::nil(),
                                            instruments: vec![instrument.clone()],
                                        });
                                    }
                                }
                                #[allow(unreachable_patterns)]
                                _ => {
                                    warn!(
                                        "Unknown market data source for instrument {}",
                                        instrument
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// 向客户端发送市场数据
    fn send_market_data_to_client(&self, client_id: &str, instrument: &str, data: &MDSnapshot) {
        if let Some(subscriber) = self.subscribers.get(client_id) {
            // 检查是否订阅了该合约
            if subscriber.instruments.contains(instrument) {
                // 检查客户端是否有该合约的快照
                let is_first_update = self
                    .client_snapshots
                    .get(client_id)
                    .map(|snapshots| !snapshots.contains_key(instrument))
                    .unwrap_or(true);

                let data_json = if is_first_update {
                    // 首次更新，发送全量数据
                    self.snapshot_to_json(data)
                } else if let Some(client_snapshots) = self.client_snapshots.get(client_id) {
                    if let Some(old_snapshot) = client_snapshots.get(instrument) {
                        // 有历史快照，计算增量
                        let changes = self.compare_snapshot(old_snapshot, data);
                        if changes.is_empty() {
                            // 没有变化，不发送
                            return;
                        }

                        // 构建增量JSON
                        let mut json_data = serde_json::Value::Object(serde_json::Map::new());
                        json_data["instrument_id"] = json!(instrument);
                        self.apply_changes_to_json(&mut json_data, &changes);
                        json_data
                    } else {
                        // 没有历史快照，发送全量
                        self.snapshot_to_json(data)
                    }
                } else {
                    // 没有客户端快照，发送全量
                    self.snapshot_to_json(data)
                };

                // 构建市场数据更新消息
                let mut data_map = HashMap::new();
                data_map.insert(instrument.to_string(), data_json.to_string());

                let message = MarketDataUpdateMessage {
                    instruments: vec![instrument.to_string()],
                    data: data_map,
                };

                // 发送给订阅者
                match subscriber.addr.try_send(message) {
                    Err(e) => error!("Failed to send market data to client {}: {}", client_id, e),
                    _ => {}
                }
            }
        }
    }

    /// 发送市场数据更新
    fn broadcast_market_data(&self, data: &MDSnapshot) {
        let instrument = &data.instrument_id;

        // 获取订阅该合约的客户端列表
        if let Some(subscribers) = self.instrument_subscribers.get(instrument) {
            for client_id in subscribers {
                self.send_market_data_to_client(client_id, instrument, data);
            }
        }
    }

    /// 查找合适的Actor处理订阅请求
    fn find_actor_for_instrument(
        &self,
        instrument: &str,
    ) -> Option<(
        Addr<crate::actors::md_actor::MarketDataActor>,
        MarketDataSource,
    )> {
        // 首先检查该合约是否已经有数据源
        if let Some(source) = self.source_map.get(instrument) {
            match source {
                MarketDataSource::CTP => {
                    if let Some((_, actor)) = self.ctp_actors.iter().next() {
                        return Some((actor.clone(), MarketDataSource::CTP));
                    }
                }

                // MarketDataSource::QQ => {
                //     if let Some((_, actor)) = self.qq_actors.iter().next() {
                //         return Some((actor.clone(), MarketDataSource::QQ));
                //     }
                // }
                // MarketDataSource::Sina => {
                //     if let Some((_, actor)) = self.sina_actors.iter().next() {
                //         return Some((actor.clone(), MarketDataSource::Sina));
                //     }
                // }
                #[allow(unreachable_patterns)]
                _ => {}
            }
        }

        // 简化：由于构建时只会启用一个feature，直接返回对应类型的第一个actor即可

        if let Some((_, actor)) = self.ctp_actors.iter().next() {
            return Some((actor.clone(), MarketDataSource::CTP));
        }

        #[cfg(feature = "qq")]
        if let Some((_, actor)) = self.qq_actors.iter().next() {
            return Some((actor.clone(), MarketDataSource::QQ));
        }

        #[cfg(feature = "sina")]
        if let Some((_, actor)) = self.sina_actors.iter().next() {
            return Some((actor.clone(), MarketDataSource::Sina));
        }

        // 没有找到合适的数据源
        warn!(
            "No suitable market data actor found for instrument: {}",
            instrument
        );
        None
    }

    /// 检查两个快照之间的字段变化，返回变化的字段及值
    fn compare_snapshot(
        &self,
        old_data: &MDSnapshot,
        new_data: &MDSnapshot,
    ) -> HashMap<String, serde_json::Value> {
        let mut changes = HashMap::new();

        // 检查所有字段是否有变化
        if old_data.last_price != new_data.last_price {
            changes.insert("last_price".to_string(), json!(new_data.last_price));
        }

        if old_data.pre_settlement != new_data.pre_settlement {
            changes.insert("pre_settlement".to_string(), json!(new_data.pre_settlement));
        }

        if old_data.pre_close != new_data.pre_close {
            changes.insert("pre_close".to_string(), json!(new_data.pre_close));
        }

        if old_data.pre_open_interest != new_data.pre_open_interest {
            changes.insert(
                "pre_open_interest".to_string(),
                json!(new_data.pre_open_interest),
            );
        }

        if old_data.open != new_data.open {
            changes.insert("open".to_string(), json!(new_data.open));
        }

        if old_data.highest != new_data.highest {
            changes.insert("highest".to_string(), json!(new_data.highest));
        }

        if old_data.lowest != new_data.lowest {
            changes.insert("lowest".to_string(), json!(new_data.lowest));
        }

        if old_data.volume != new_data.volume {
            changes.insert("volume".to_string(), json!(new_data.volume));
        }

        if old_data.amount != new_data.amount {
            changes.insert("amount".to_string(), json!(new_data.amount));
        }

        if old_data.open_interest != new_data.open_interest {
            changes.insert("open_interest".to_string(), json!(new_data.open_interest));
        }

        if old_data.close != new_data.close {
            changes.insert("close".to_string(), json!(new_data.close));
        }

        if old_data.settlement != new_data.settlement {
            changes.insert("settlement".to_string(), json!(new_data.settlement));
        }

        if old_data.upper_limit != new_data.upper_limit {
            changes.insert("upper_limit".to_string(), json!(new_data.upper_limit));
        }

        if old_data.lower_limit != new_data.lower_limit {
            changes.insert("lower_limit".to_string(), json!(new_data.lower_limit));
        }

        if old_data.bid_price1 != new_data.bid_price1 {
            changes.insert("bid_price1".to_string(), json!(new_data.bid_price1));
        }

        if old_data.bid_volume1 != new_data.bid_volume1 {
            changes.insert("bid_volume1".to_string(), json!(new_data.bid_volume1));
        }

        if old_data.ask_price1 != new_data.ask_price1 {
            changes.insert("ask_price1".to_string(), json!(new_data.ask_price1));
        }

        if old_data.ask_volume1 != new_data.ask_volume1 {
            changes.insert("ask_volume1".to_string(), json!(new_data.ask_volume1));
        }

        if old_data.bid_price2 != new_data.bid_price2 {
            changes.insert("bid_price2".to_string(), json!(new_data.bid_price2));
        }

        if old_data.bid_volume2 != new_data.bid_volume2 {
            changes.insert("bid_volume2".to_string(), json!(new_data.bid_volume2));
        }

        if old_data.ask_price2 != new_data.ask_price2 {
            changes.insert("ask_price2".to_string(), json!(new_data.ask_price2));
        }

        if old_data.ask_volume2 != new_data.ask_volume2 {
            changes.insert("ask_volume2".to_string(), json!(new_data.ask_volume2));
        }

        if old_data.bid_price3 != new_data.bid_price3 {
            changes.insert("bid_price3".to_string(), json!(new_data.bid_price3));
        }

        if old_data.bid_volume3 != new_data.bid_volume3 {
            changes.insert("bid_volume3".to_string(), json!(new_data.bid_volume3));
        }

        if old_data.ask_price3 != new_data.ask_price3 {
            changes.insert("ask_price3".to_string(), json!(new_data.ask_price3));
        }

        if old_data.ask_volume3 != new_data.ask_volume3 {
            changes.insert("ask_volume3".to_string(), json!(new_data.ask_volume3));
        }

        if old_data.bid_price4 != new_data.bid_price4 {
            changes.insert("bid_price4".to_string(), json!(new_data.bid_price4));
        }

        if old_data.bid_volume4 != new_data.bid_volume4 {
            changes.insert("bid_volume4".to_string(), json!(new_data.bid_volume4));
        }

        if old_data.ask_price4 != new_data.ask_price4 {
            changes.insert("ask_price4".to_string(), json!(new_data.ask_price4));
        }

        if old_data.ask_volume4 != new_data.ask_volume4 {
            changes.insert("ask_volume4".to_string(), json!(new_data.ask_volume4));
        }

        if old_data.bid_price5 != new_data.bid_price5 {
            changes.insert("bid_price5".to_string(), json!(new_data.bid_price5));
        }

        if old_data.bid_volume5 != new_data.bid_volume5 {
            changes.insert("bid_volume5".to_string(), json!(new_data.bid_volume5));
        }

        if old_data.ask_price5 != new_data.ask_price5 {
            changes.insert("ask_price5".to_string(), json!(new_data.ask_price5));
        }

        if old_data.ask_volume5 != new_data.ask_volume5 {
            changes.insert("ask_volume5".to_string(), json!(new_data.ask_volume5));
        }

        if old_data.average != new_data.average {
            changes.insert("average".to_string(), json!(new_data.average));
        }

        if old_data.datetime != new_data.datetime {
            changes.insert("datetime".to_string(), json!(new_data.datetime.clone()));
        }

        changes
    }

    /// 将数据转换为完整的JSON
    fn snapshot_to_json(&self, data: &MDSnapshot) -> serde_json::Value {
        json!({
            "instrument_id": data.instrument_id.clone(),
            "last_price": data.last_price,
            "pre_settlement": data.pre_settlement,
            "pre_close": data.pre_close,
            "pre_open_interest": data.pre_open_interest,
            "open": data.open,
            "highest": data.highest,
            "lowest": data.lowest,
            "volume": data.volume,
            "amount": data.amount,
            "open_interest": data.open_interest,
            "close": data.close,
            "settlement": data.settlement,
            "upper_limit": data.upper_limit,
            "lower_limit": data.lower_limit,
            "bid_price1": data.bid_price1,
            "bid_volume1": data.bid_volume1,
            "ask_price1": data.ask_price1,
            "ask_volume1": data.ask_volume1,
            "bid_price2": data.bid_price2,
            "bid_volume2": data.bid_volume2,
            "ask_price2": data.ask_price2,
            "ask_volume2": data.ask_volume2,
            "bid_price3": data.bid_price3,
            "bid_volume3": data.bid_volume3,
            "ask_price3": data.ask_price3,
            "ask_volume3": data.ask_volume3,
            "bid_price4": data.bid_price4,
            "bid_volume4": data.bid_volume4,
            "ask_price4": data.ask_price4,
            "ask_volume4": data.ask_volume4,
            "bid_price5": data.bid_price5,
            "bid_volume5": data.bid_volume5,
            "ask_price5": data.ask_price5,
            "ask_volume5": data.ask_volume5,
            "average": data.average,
            "datetime": data.datetime.clone()
        })
    }

    /// 应用增量更新到JSON数据
    fn apply_changes_to_json(
        &self,
        json_data: &mut serde_json::Value,
        changes: &HashMap<String, serde_json::Value>,
    ) {
        if let serde_json::Value::Object(obj) = json_data {
            for (field, value) in changes {
                obj.insert(field.clone(), value.clone());
            }
        }
    }

    /// 是否满足批量发送的条件
    fn should_send_batch(&self) -> bool {
        Instant::now().duration_since(self.last_batch_send) > self.batch_interval
            || self.batch_updates.len() >= self.batch_size_threshold
    }

    /// 发送批量增量更新
    fn send_batch_updates(&mut self) {
        if self.batch_updates.is_empty() {
            return;
        }

        // 获取所有有更新的合约
        let instruments_with_updates: HashSet<String> =
            self.batch_updates.keys().cloned().collect();

        // 遍历所有客户端，发送订阅的更新
        for (client_id, subscriber) in &self.subscribers {
            // 找出该客户端订阅的且有更新的合约
            let client_instruments: HashSet<_> = subscriber
                .instruments
                .intersection(&instruments_with_updates)
                .collect();

            if client_instruments.is_empty() {
                continue;
            }

            // 为客户端构建更新消息
            let mut data_map = HashMap::new();
            let mut update_instruments = Vec::new();

            for &instrument in &client_instruments {
                if let Some(changes) = self.batch_updates.get(instrument) {
                    if changes.is_empty() {
                        continue;
                    }

                    // 为每个合约构造增量JSON
                    let mut instrument_data = serde_json::Value::Object(serde_json::Map::new());

                    // 确保instrument_id字段始终存在
                    if !changes.contains_key("instrument_id") {
                        instrument_data["instrument_id"] = json!(instrument);
                    }

                    // 应用所有变化
                    self.apply_changes_to_json(&mut instrument_data, changes);

                    // 添加到数据映射
                    data_map.insert(instrument.to_string(), instrument_data.to_string());
                    update_instruments.push(instrument.to_string());

                    // 更新客户端快照
                    if let Some(market_data) = self.market_data_cache.get(instrument) {
                        self.client_snapshots
                            .entry(client_id.clone())
                            .or_insert_with(HashMap::new)
                            .insert(instrument.to_string(), market_data.clone());
                    }
                }
            }

            if !update_instruments.is_empty() {
                // 发送增量更新
                let message = MarketDataUpdateMessage {
                    instruments: update_instruments,
                    data: data_map,
                };

                if let Err(e) = subscriber.addr.try_send(message) {
                    error!("Failed to send batch update to client {}: {}", client_id, e);
                } else {
                    debug!("Sent incremental update to client {}", client_id);
                }
            }
        }

        // 清除批量更新缓存
        self.batch_updates.clear();
        self.last_batch_send = Instant::now();
    }
}

// 处理市场数据更新消息
impl Handler<MarketDataUpdate> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: MarketDataUpdate, _: &mut Self::Context) -> Self::Result {
        let (data, source) = (msg.0, msg.1);
        let instrument = data.instrument_id.clone();

        // 检查是否需要计算增量更新
        let mut changes = HashMap::new();
        if let Some(old_data) = self.market_data_cache.get(&instrument) {
            // 计算变化的字段
            changes = self.compare_snapshot(old_data, &data);

            // 如果没有变化，就不需要更新
            if changes.is_empty() {
                return;
            }
        } else {
            // 新合约，所有字段都是变化的
            changes = self
                .snapshot_to_json(&data)
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
        }
        debug!("MD_snapshort: {:?}", data);
        // 更新缓存
        self.market_data_cache
            .insert(instrument.clone(), data.clone());
        self.source_map.insert(instrument.clone(), source);

        // 添加到批量更新缓存
        self.batch_updates.insert(instrument.clone(), changes);

        // 检查是否应立即发送批量更新
        if self.should_send_batch() {
            self.send_batch_updates();
        }
    }
}

// 处理客户端注册消息
impl Handler<RegisterDataReceiver> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: RegisterDataReceiver, _: &mut Self::Context) -> Self::Result {
        let client_id = msg.client_id.clone();

        // 创建新的订阅者
        let subscriber = Subscriber {
            addr: msg.addr,
            instruments: HashSet::new(),
        };

        // 保存订阅者信息
        self.subscribers.insert(client_id.clone(), subscriber);

        // 创建客户端快照存储
        self.client_snapshots
            .entry(client_id.clone())
            .or_insert_with(HashMap::new);

        // 添加订阅
        if !msg.instruments.is_empty() {
            self.add_subscription(&client_id, &msg.instruments);

            // 处理每个合约的订阅
            for instrument in &msg.instruments {
                // 查找合适的Actor处理订阅请求
                if let Some((actor, source)) = self.find_actor_for_instrument(instrument) {
                    // 记录数据源
                    self.source_map.insert(instrument.clone(), source);

                    // 发送订阅请求
                    actor.do_send(Subscribe {
                        id: uuid::Uuid::nil(),
                        instruments: vec![instrument.clone()],
                    });
                } else {
                    warn!(
                        "No suitable market data actor found for instrument {}",
                        instrument
                    );
                }
            }
        }
    }
}

// 处理客户端取消注册消息
impl Handler<UnregisterDataReceiver> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: UnregisterDataReceiver, _: &mut Self::Context) -> Self::Result {
        if let Some(subscriber) = self.subscribers.remove(&msg.client_id) {
            // 获取客户端订阅的所有合约
            let instruments: Vec<String> = subscriber.instruments.into_iter().collect();

            // 移除订阅
            if !instruments.is_empty() {
                self.remove_subscription(&msg.client_id, &instruments);
            }

            // 清理客户端快照
            self.client_snapshots.remove(&msg.client_id);
        }
    }
}

// 处理订阅更新消息
impl Handler<UpdateSubscription> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: UpdateSubscription, _: &mut Self::Context) -> Self::Result {
        if let Some(subscriber) = self.subscribers.get(&msg.client_id) {
            // 获取当前订阅的合约列表
            let current_instruments: HashSet<String> = subscriber.instruments.clone();

            // 计算需要添加的合约
            let new_instruments: HashSet<String> = msg.instruments.iter().cloned().collect();
            let to_add: Vec<String> = new_instruments
                .difference(&current_instruments)
                .cloned()
                .collect();

            // 计算需要移除的合约
            let to_remove: Vec<String> = current_instruments
                .difference(&new_instruments)
                .cloned()
                .collect();

            // 添加新订阅
            if !to_add.is_empty() {
                self.add_subscription(&msg.client_id, &to_add);

                // 处理每个合约的订阅
                for instrument in &to_add {
                    // 查找合适的Actor处理订阅请求
                    if let Some((actor, source)) = self.find_actor_for_instrument(instrument) {
                        // 记录数据源
                        self.source_map.insert(instrument.clone(), source);

                        // 发送订阅请求
                        actor.do_send(Subscribe {
                            id: uuid::Uuid::nil(),
                            instruments: vec![instrument.clone()],
                        });
                    } else {
                        warn!(
                            "No suitable market data actor found for instrument {}",
                            instrument
                        );
                    }
                }
            }

            // 移除旧订阅
            if !to_remove.is_empty() {
                self.remove_subscription(&msg.client_id, &to_remove);
            }
        }
    }
}

// 处理订阅查询消息
impl Handler<QuerySubscription> for MarketDataDistributor {
    type Result = Vec<String>;

    fn handle(&mut self, msg: QuerySubscription, _: &mut Self::Context) -> Self::Result {
        if let Some(subscriber) = self.subscribers.get(&msg.client_id) {
            subscriber.instruments.iter().cloned().collect()
        } else {
            Vec::new()
        }
    }
}

// 处理CTP市场数据Actor注册消息
#[cfg(feature = "ctp")]
impl Handler<RegisterCTPMdActor> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: RegisterCTPMdActor, ctx: &mut Self::Context) -> Self::Result {
        // 注册CTP市场数据Actor
        let broker_id = msg.broker_id.clone();
        self.ctp_actors.insert(broker_id.clone(), msg.addr.clone());

        // 将分发器地址注册到Actor
        msg.addr.do_send(RegisterDistributor {
            addr: ctx.address(),
        });

        info!("Registered CTP market data actor for broker {}", broker_id);
    }
}

// 处理QQ市场数据Actor注册消息
#[cfg(feature = "qq")]
impl Handler<RegisterQQMdActor> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: RegisterQQMdActor, ctx: &mut Self::Context) -> Self::Result {
        // 注册QQ市场数据Actor
        let broker_id = msg.broker_id.clone();
        self.qq_actors.insert(broker_id.clone(), msg.addr.clone());

        // 将分发器地址注册到Actor
        msg.addr.do_send(RegisterDistributor {
            addr: ctx.address(),
        });

        info!("Registered QQ market data actor for broker {}", broker_id);
    }
}

// 处理Sina市场数据Actor注册消息
#[cfg(feature = "sina")]
impl Handler<RegisterSinaMdActor> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: RegisterSinaMdActor, ctx: &mut Self::Context) -> Self::Result {
        // 注册Sina市场数据Actor
        let broker_id = msg.broker_id.clone();
        self.sina_actors.insert(broker_id.clone(), msg.addr.clone());

        // 将分发器地址注册到Actor
        msg.addr.do_send(RegisterDistributor {
            addr: ctx.address(),
        });

        info!("Registered Sina market data actor for broker {}", broker_id);
    }
}

// 处理通用市场数据Actor注册消息
impl Handler<RegisterMdActor> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: RegisterMdActor, ctx: &mut Self::Context) -> Self::Result {
        // 根据数据源类型注册到不同的集合
        let broker_id = msg.broker_id.clone();
        match msg.source_type {
            // #[cfg(feature = "ctp")]
            MarketDataSource::CTP => {
                self.ctp_actors.insert(broker_id.clone(), msg.addr.clone());
                info!("Registered CTP market data actor for broker {}", broker_id);
            }
            // #[cfg(feature = "qq")]
            // MarketDataSource::QQ => {
            //     self.qq_actors.insert(broker_id.clone(), msg.addr.clone());
            //     info!("Registered QQ market data actor for broker {}", broker_id);
            // }
            // #[cfg(feature = "sina")]
            // MarketDataSource::Sina => {
            //     self.sina_actors.insert(broker_id.clone(), msg.addr.clone());
            //     info!("Registered Sina market data actor for broker {}", broker_id);
            // }
            #[allow(unreachable_patterns)]
            _ => {
                warn!("Unknown market data source type {:?}", msg.source_type);
            }
        }

        // 将分发器地址注册到Actor
        msg.addr.do_send(RegisterDistributor {
            addr: ctx.address(),
        });
    }
}

// 处理获取所有订阅的消息
impl Handler<GetAllSubscriptions> for MarketDataDistributor {
    type Result = Vec<String>;

    fn handle(&mut self, _: GetAllSubscriptions, _: &mut Self::Context) -> Self::Result {
        // 返回所有已订阅的合约列表
        let mut result = HashSet::new();

        // 从所有合约订阅关系中收集合约
        for instrument in self.instrument_subscribers.keys() {
            result.insert(instrument.clone());
        }

        result.into_iter().collect()
    }
}

// 处理添加单个订阅消息
impl Handler<AddSubscription> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: AddSubscription, _: &mut Self::Context) -> Self::Result {
        // 添加订阅
        self.add_subscription(&msg.client_id.to_string(), &[msg.instrument.clone()]);
    }
}

// 处理移除单个订阅消息
impl Handler<RemoveSubscription> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: RemoveSubscription, _: &mut Self::Context) -> Self::Result {
        // 移除订阅
        self.remove_subscription(&msg.client_id.to_string(), &[msg.instrument.clone()]);
    }
}
