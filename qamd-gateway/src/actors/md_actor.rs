use actix::prelude::*;
use ctp2rs::ffi::{AssignFromString, WrapToString};
use ctp2rs::v1alpha1::{
    CThostFtdcDepthMarketDataField, CThostFtdcReqUserLoginField, CThostFtdcRspInfoField,
    CThostFtdcRspUserLoginField, CThostFtdcSpecificInstrumentField,
};

use ctp2rs::v1alpha1::{MdApi, MdSpi};

use log::{debug, error, info, warn};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// 统一导入消息类型
use crate::actors::messages::*;
use crate::config::BrokerConfig;
use crate::converter::convert_ctp_to_md_snapshot;

// 统一的SPI实现，用于回调处理
struct MarketDataSpiImpl {
    name: String,
    // 使用actor的地址将消息从CTP回调发送回actor
    actor_addr: Addr<MarketDataActor>,
    subscribed_instruments: Arc<Mutex<HashSet<String>>>,
}

// SPI接口实现，处理所有回调
impl MdSpi for MarketDataSpiImpl {
    fn on_front_connected(&mut self) {
        info!("MD Front {} connected", self.name);
        self.actor_addr.do_send(MarketDataEvent::Connected);
    }

    fn on_front_disconnected(&mut self, reason: i32) {
        warn!("MD Front {} disconnected: {:?}", self.name, reason);
        self.actor_addr.do_send(MarketDataEvent::Disconnected);
    }

    fn on_rsp_user_login(
        &mut self,
        rsp_user_login: Option<&CThostFtdcRspUserLoginField>,
        rsp_info: Option<&CThostFtdcRspInfoField>,
        request_id: i32,
        is_last: bool,
    ) {
        info!(
            "Login {} response: RequestID={}, IsLast={}",
            self.name, request_id, is_last
        );

        if let Some(login_info) = rsp_user_login {
            let trading_day = login_info.TradingDay.to_string();
            let login_time = login_info.LoginTime.to_string();
            let broker_id = login_info.BrokerID.to_string();
            let user_id = login_info.UserID.to_string();

            info!(
                "MD Logged in: Trading Day = {}, Login Time = {}, Broker ID = {}, User ID = {}",
                trading_day, login_time, broker_id, user_id
            );

            self.actor_addr.do_send(MarketDataEvent::LoggedIn);
        } else if let Some(error) = rsp_info {
            let error_msg = format!(
                "MD Login {} failed: Error = {}",
                self.name,
                error.ErrorMsg.to_string()
            );
            error!("{}", error_msg);
            self.actor_addr.do_send(MarketDataEvent::Error(error_msg));
        }
    }

    fn on_rsp_sub_market_data(
        &mut self,
        specific_instrument: Option<&CThostFtdcSpecificInstrumentField>,
        rsp_info: Option<&CThostFtdcRspInfoField>,
        request_id: i32,
        is_last: bool,
    ) {
        info!(
            "Subscribe response: RequestID={}, IsLast={}",
            request_id, is_last
        );

        if let Some(instrument) = specific_instrument {
            let instrument_id = instrument.InstrumentID.to_string();

            if rsp_info.unwrap().ErrorID == 0 {
                info!("Subscribed to market data for {}", instrument_id);

                // 保存订阅信息
                if let Ok(mut subscribed) = self.subscribed_instruments.lock() {
                    subscribed.insert(instrument_id.clone());
                }

                self.actor_addr
                    .do_send(MarketDataEvent::SubscriptionSuccess(instrument_id));
            } else if let Some(error) = rsp_info {
                let error_msg = format!(
                    "Failed to subscribe to market data for {}: Error = {}",
                    instrument_id,
                    error.ErrorMsg.to_string()
                );
                error!("{}", error_msg);
                self.actor_addr
                    .do_send(MarketDataEvent::SubscriptionFailure(
                        instrument_id,
                        error_msg,
                    ));
            }
        }
    }

    fn on_rtn_depth_market_data(
        &mut self,
        depth_market_data: Option<&CThostFtdcDepthMarketDataField>,
    ) {
        if let Some(market_data) = depth_market_data {
            // 将数据克隆后发送给actor
            let market_data_owned: CThostFtdcDepthMarketDataField = *market_data;
            self.actor_addr
                .do_send(MarketDataEvent::MarketData(market_data_owned));
        }
    }

    fn on_rsp_unsub_market_data(
        &mut self,
        specific_instrument: Option<&CThostFtdcSpecificInstrumentField>,
        result: Option<&CThostFtdcRspInfoField>,
        request_id: i32,
        is_last: bool,
    ) {
        info!(
            "Unsubscribe response: RequestID={}, IsLast={}",
            request_id, is_last
        );

        if let Some(instrument) = specific_instrument {
            let instrument_id = instrument.InstrumentID.to_string();

            if result.unwrap().ErrorID == 0 {
                info!("Unsubscribed from market data for {}", instrument_id);

                // 移除订阅信息
                if let Ok(mut subscribed) = self.subscribed_instruments.lock() {
                    subscribed.remove(&instrument_id);
                }
            } else if let Some(error) = result {
                error!(
                    "Failed to unsubscribe from market data for {}: Error = {}",
                    instrument_id,
                    error.ErrorMsg.to_string()
                );
            }
        }
    }

    fn on_rsp_error(
        &mut self,
        result: Option<&CThostFtdcRspInfoField>,
        request_id: i32,
        is_last: bool,
    ) {
        if let Some(error) = result {
            let error_msg = format!(
                "MD error: Request ID = {}, Is Last = {}, Error = {}",
                request_id,
                is_last,
                error.ErrorMsg.to_string()
            );
            error!("{}", error_msg);
            self.actor_addr.do_send(MarketDataEvent::Error(error_msg));
        }
    }
}

// 统一的MarketDataActor结构，通过feature flags选择实际的API实现
pub struct MarketDataActor {
    // #[cfg(feature = "ctp")]
    md_api: Option<ctp2rs::v1alpha1::MdApi>,
    subscribed_instruments: Arc<Mutex<HashSet<String>>>,
    broker_config: BrokerConfig,
    distributor: Option<Addr<crate::actors::md_distributor::MarketDataDistributor>>,
    front_addr: String,
    dynlib_path: String,
    user_id: String,
    password: String,
    broker_id: String,
    is_connected: bool,
    is_logged_in: bool,
    source_type: MarketDataSource,
}

impl Actor for MarketDataActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!(
            "MarketDataActor [{}:{}] started",
            &self.broker_config.name, &self.broker_id
        );

        // 调度心跳以检查连接状态
        ctx.run_interval(Duration::from_secs(30), |act, ctx| {
            if !act.is_connected {
                info!(
                    "MarketDataActor  [{}:{}]  heartbeat: Not connected, attempting to reconnect",
                    act.broker_config.name, act.broker_id
                );
                act.init_md_api(ctx);
            }
        });
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        info!(
            "MarketDataActor  [{}:{}]  stopped",
            self.broker_config.name, self.broker_id
        );
    }
}

impl MarketDataActor {
    // 创建新的市场数据Actor，可根据编译时特性决定具体行为
    // #[cfg(feature = "ctp")]
    pub fn new(config: BrokerConfig) -> Self {
        let front_addr = config.front_addr.clone();
        let user_id = config.user_id.clone();
        let password = config.password.clone();
        let broker_id = config.broker_id.clone();
        let dynlib_path: String = config.dynlib_path.clone();

        let source_type = match config.source_type {
            Some(ref x) if x.to_lowercase() == "ctp" => MarketDataSource::CTP,
            Some(ref x) if x.to_lowercase() == "sina" => MarketDataSource::Sina,
            Some(ref x) if x.to_lowercase() == "qq" => MarketDataSource::QQ,
            _ => MarketDataSource::CTP,
        };

        Self {
            md_api: None,
            subscribed_instruments: Arc::new(Mutex::new(HashSet::new())),
            broker_config: config,
            distributor: None,
            front_addr,
            dynlib_path,
            user_id,
            password,
            broker_id,
            is_connected: false,
            is_logged_in: false,
            source_type: source_type,
        }
    }

    // 初始化市场数据API，根据编译时特性选择不同实现
    fn init_md_api(&mut self, ctx: &mut Context<Self>) {
        // #[cfg(feature = "ctp")]
        {
            // 创建CTP的MdApi
            let md_api = MdApi::create_api(&self.dynlib_path, "md_", false, false);

            // 创建SPI并注册
            let addr = ctx.address();
            let subscribed_instruments = self.subscribed_instruments.clone();
            let spi: Box<MarketDataSpiImpl> = Box::new(MarketDataSpiImpl {
                name: self.broker_config.name.clone(),
                actor_addr: addr,
                subscribed_instruments,
            });
            let spi_ptr = Box::into_raw(spi) as *mut dyn MdSpi;

            md_api.register_spi(spi_ptr);

            // 连接
            md_api.register_front(&self.front_addr);

            // 初始化API
            md_api.init();
            std::thread::sleep(std::time::Duration::from_secs(1));

            // 保存API
            self.md_api = Some(md_api);
        }
    }

    // 登录方法，根据编译时特性选择不同实现
    fn login(&mut self) -> Result<(), String> {
        // #[cfg(any(feature = "ctp", feature = "qq", feature = "sina"))]
        if let Some(ref mut md_api) = self.md_api {
            let mut req = CThostFtdcReqUserLoginField::default();

            // 填充登录请求
            if !self.broker_id.is_empty() {
                req.BrokerID.assign_from_str(&self.broker_id);
            }

            if !self.user_id.is_empty() {
                req.UserID.assign_from_str(&self.user_id);
            }

            if !self.password.is_empty() {
                req.Password.assign_from_str(&self.password);
            }

            // 执行登录
            let result = md_api.req_user_login(&mut req, 1);

            match result {
                0 => {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    Ok(())
                }

                x => {
                    let error_msg = format!(
                        "{:?} failed to send login request: {:?}",
                        (&self.broker_config.name, &self.broker_id),
                        x
                    );
                    error!("{}", error_msg);
                    Err(error_msg)
                }
            }
        } else {
            Err(format!(
                "Market data {:?} API not initialized",
                (&self.broker_config.name, &self.broker_id)
            ))
        }
    }

    // 订阅合约方法
    fn subscribe_instruments(&mut self, instruments: &[String]) -> Result<(), String> {
        if !self.is_logged_in {
            return Err("Not logged in".to_string());
        }

        if let Some(ref mut md_api) = self.md_api {
            // 将合约ID转换为CString

            // 执行订阅
            let instruments_vec: Vec<String> = instruments.to_vec();
            let result = md_api.subscribe_market_data(&instruments_vec);
            debug!("sub {:?} ret: {}", instruments_vec, result);
            match result {
                0 => {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    Ok(())
                }
                e => Err(format!(
                    "Failed to subscribe to instruments, error: {:?}",
                    e
                )),
            }
        } else {
            Err("MD API not initialized".to_string())
        }
    }

    // 取消订阅合约方法
    fn unsubscribe_instruments(&mut self, instruments: &[String]) -> Result<(), String> {
        if !self.is_logged_in {
            return Err("Not logged in".to_string());
        }

        if let Some(ref mut md_api) = self.md_api {
            let instruments_vec: Vec<String> = instruments.to_vec();
            // 执行取消订阅
            let result = md_api.unsubscribe_market_data(&instruments_vec);

            match result {
                0 => Ok(()),
                e => Err(format!(
                    "Failed to unsubscribe from instruments, error: {:?}",
                    e
                )),
            }
        } else {
            Err("MD API not initialized".to_string())
        }
    }
}

// 统一的Actor消息处理实现
impl Handler<InitMarketDataSource> for MarketDataActor {
    type Result = ();

    fn handle(&mut self, _: InitMarketDataSource, ctx: &mut Self::Context) -> Self::Result {
        self.init_md_api(ctx);
    }
}

impl Handler<LoginMarketDataSource> for MarketDataActor {
    type Result = Result<(), String>;

    fn handle(&mut self, _: LoginMarketDataSource, _: &mut Self::Context) -> Self::Result {
        self.login()
    }
}

impl Handler<Subscribe> for MarketDataActor {
    type Result = ();

    fn handle(&mut self, msg: Subscribe, _: &mut Self::Context) -> Self::Result {
        if let Err(e) = self.subscribe_instruments(&msg.instruments) {
            error!("Failed to subscribe to instruments: {}", e);
        }
    }
}

impl Handler<Unsubscribe> for MarketDataActor {
    type Result = ();

    fn handle(&mut self, msg: Unsubscribe, _: &mut Self::Context) -> Self::Result {
        if let Err(e) = self.unsubscribe_instruments(&msg.instruments) {
            error!("Failed to unsubscribe from instruments: {}", e);
        }
    }
}

impl Handler<GetSubscriptions> for MarketDataActor {
    type Result = Vec<String>;

    fn handle(&mut self, msg: GetSubscriptions, _: &mut Self::Context) -> Self::Result {
        let subscriptions = if let Ok(subscribed) = self.subscribed_instruments.lock() {
            subscribed.iter().cloned().collect()
        } else {
            Vec::new()
        };

        // 如果提供了回调，则执行回调
        if let Some(callback) = msg.callback {
            callback(subscriptions.clone());
        }

        subscriptions
    }
}

impl Handler<MarketDataEvent> for MarketDataActor {
    type Result = ();

    fn handle(&mut self, msg: MarketDataEvent, _: &mut Self::Context) -> Self::Result {
        match msg {
            MarketDataEvent::Connected => {
                info!("Market data source connected");
                self.is_connected = true;

                // 连接后自动登录
                if let Err(e) = self.login() {
                    error!("Failed to login: {}", e);
                }
            }
            MarketDataEvent::Disconnected => {
                warn!("Market data source disconnected");
                self.is_connected = false;
                self.is_logged_in = false;
            }
            MarketDataEvent::LoggedIn => {
                info!("Market data source logged in");
                self.is_logged_in = true;

                // 重新订阅所有合约
                let instruments = {
                    if let Ok(subscribed) = self.subscribed_instruments.lock() {
                        subscribed.iter().cloned().collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    }
                };

                if !instruments.is_empty() {
                    if let Err(e) = self.subscribe_instruments(&instruments) {
                        error!("Failed to resubscribe to instruments: {}", e);
                    }
                }
            }
            MarketDataEvent::MarketData(md) => {
                // 转换为MDSnapshot
                match convert_ctp_to_md_snapshot(&md) {
                    Ok(snapshot) => {
                        debug!("Received market data for {}", snapshot.instrument_id);
                        // 转发给distributor
                        if let Some(distributor) = &self.distributor {
                            distributor.do_send(MarketDataUpdate(snapshot, self.source_type));
                        }
                    }
                    Err(e) => {
                        error!("Failed to convert market data: {}", e);
                    }
                }
            }
            MarketDataEvent::SubscriptionSuccess(instrument) => {
                info!("Successfully subscribed to {}", instrument);
            }
            MarketDataEvent::SubscriptionFailure(instrument, error) => {
                error!("Failed to subscribe to {}: {}", instrument, error);
            }
            MarketDataEvent::Error(error) => {
                error!("Market data error: {}", error);
            }
        }
    }
}

impl Handler<RegisterDistributor> for MarketDataActor {
    type Result = ();

    fn handle(&mut self, msg: RegisterDistributor, _: &mut Self::Context) -> Self::Result {
        self.distributor = Some(msg.addr);
        info!("Market data distributor registered");
    }
}

impl Handler<StartMarketData> for MarketDataActor {
    type Result = ();

    fn handle(&mut self, msg: StartMarketData, ctx: &mut Self::Context) -> Self::Result {
        // 如果API未初始化，则初始化
        if self.md_api.is_none() {
            self.init_md_api(ctx);
        }

        // 订阅合约
        if !msg.instruments.is_empty() {
            if let Err(e) = self.subscribe_instruments(&msg.instruments) {
                error!("Failed to subscribe to initial instruments: {}", e);
            }
        }
    }
}

impl Handler<StopMarketData> for MarketDataActor {
    type Result = ();

    fn handle(&mut self, _: StopMarketData, _: &mut Self::Context) -> Self::Result {
        // 取消订阅所有合约
        let instruments = {
            if let Ok(subscribed) = self.subscribed_instruments.lock() {
                subscribed.iter().cloned().collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        };

        if !instruments.is_empty() {
            if let Err(e) = self.unsubscribe_instruments(&instruments) {
                error!("Failed to unsubscribe from instruments: {}", e);
            }
        }
    }
}

impl Handler<RestartActor> for MarketDataActor {
    type Result = ();

    fn handle(&mut self, _: RestartActor, ctx: &mut Self::Context) -> Self::Result {
        // 只有未连接或未登录时才重启
        if !self.is_connected || !self.is_logged_in {
            info!("Restarting market data actor for broker {}", self.broker_id);

            // 重新初始化
            if self.md_api.is_none() {
                self.init_md_api(ctx);
            }

            // 尝试重新登录
            if let Err(e) = self.login() {
                error!("Failed to login during restart: {}", e);
            }
        }
    }
}
