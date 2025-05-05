use log::{info, error};
use std::collections::HashMap;
use std::time::Duration;

use actix::prelude::*;
use uuid::Uuid;


// use crate::actors::prelude::*;
use crate::actors::messages::*;
use crate::actors::md_actor::MarketDataActor;
use crate::actors::md_distributor::MarketDataDistributor;
use crate::config::BrokerConfig;




// Conditionally import feature-specific types
// #[cfg(feature = "qq")]
// use crate::actors::qq_md_actor;

// #[cfg(feature = "sina")]
// use crate::actors::sina_md_actor;


/// 数据源类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MarketDataSourceType {
    CTP,
    QQ,
    Sina,
    // 后续可以添加更多的数据源类型
}

/// Market data connector that manages connections to market data sources
pub struct MarketDataConnector {
    /// Market data sources by ID (CTP行情源)
    md_sources: HashMap<(String, String), Addr<MarketDataActor>>,
    
    /// Market data distributor
    distributor: Addr<MarketDataDistributor>,
    /// Broker configurations
    broker_configs: Vec<BrokerConfig>,
    /// Default subscriptions
    default_subscriptions: Vec<String>,
    /// Connected clients
    clients: HashMap<Uuid, Recipient<MarketDataUpdate>>,
}

impl Actor for MarketDataConnector {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("MarketDataConnector started");
        
        // Start monitoring the market data sources
        ctx.run_interval(Duration::from_secs(60), |act, _| {
            act.check_connections();
        });
        
        // Initialize market data sources
        self.init_market_data_sources(ctx);
    }
}

impl MarketDataConnector {
    pub fn new(
        broker_configs: Vec<BrokerConfig>,
        default_subscriptions: Vec<String>,
        distributor: Addr<MarketDataDistributor>,
    ) -> Self {
        Self {
            md_sources: HashMap::new(),
            distributor,
            broker_configs,
            default_subscriptions,
            clients: HashMap::new(),
        }
    }
    
    fn init_market_data_sources(&mut self, ctx: &mut Context<Self>) {
        info!("Initializing market data sources");
        
        // Create a market data actor for each broker
        println!("broker_configs: {:?}", self.broker_configs);
        for broker_config in &self.broker_configs {
            let broker_id = broker_config.broker_id.clone();
            let broker_name = broker_config.name.clone() ;
            info!("Creating market data source for broker [{}:{}]", broker_name,  broker_id);
            
            // Create the actor
            let md_actor: Addr<MarketDataActor> = MarketDataActor::new(broker_config.clone()).start();
            
            // Store the actor
            self.md_sources.insert((broker_name, broker_id), md_actor);
        }
        
        // Initialize the market data sources
        for (broker_id, md_actor) in &self.md_sources {
            info!("Initializing market data source for broker {:?}", broker_id);
            md_actor.do_send(InitMarketDataSource);
        
            info!("Registering distributor with market data source for broker {:?}", broker_id);
            md_actor.do_send(RegisterDistributor {
                addr: self.distributor.clone(),
            });
        }
        
        // Set up periodic synchronization of subscriptions
        ctx.run_interval(Duration::from_secs(30), |act, ctx| {
            act.sync_subscriptions(ctx);
        });
    }
    
    fn start_market_data(&self) {
        if !self.default_subscriptions.is_empty() {
            info!("Starting market data with default subscriptions: {:?}", self.default_subscriptions);
            
            // Start all market data sources with default subscriptions
            for (broker_id, md_actor) in &self.md_sources {
                info!("Starting market data for broker {:?}", broker_id);
                
                md_actor.do_send(StartMarketData {
                    instruments: self.default_subscriptions.clone(),
                });
            }
        }
    }
    
    fn check_connections(&self) {
        // Check the status of each market data source
        for (broker_id, md_actor) in &self.md_sources {
            info!("Checking connection for broker {:?}", broker_id);
            md_actor.do_send(RestartActor);
        }
        
        // 检查QQ行情源

    }
    
    // Sync broker subscriptions with client subscriptions
    fn sync_subscriptions(&self, ctx: &mut Context<Self>) {
        // Get all market data sources
        let md_sources: Vec<((String, String), Addr<MarketDataActor>)> = self.md_sources
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
            
        // Clone the distributor address for use in futures
        let distributor = self.distributor.clone();
        
        // First get all active subscriptions from distributor
        let future = distributor
            .send(GetAllSubscriptions {})
            .into_actor(self)
            .map(move |result, _act, _ctx| {
                if let Ok(active_subscriptions) = result {
                    // Process each market data source
                    for (broker_id, md_actor) in md_sources {
                        // Clone active_subscriptions for this iteration
                        let active_subs = active_subscriptions.clone();
                        let broker_id_clone = broker_id.clone();
                        let md_actor_clone = md_actor.clone();
                        
                        // Using do_send instead of send+wait to avoid blocking
                        md_actor.do_send(GetSubscriptions { 
                            id: Uuid::new_v4(),
                            // Process the result in another message
                            callback: Some(Box::new(move |current_subscriptions| {
                                // Find instruments that need to be subscribed
                                let to_subscribe: Vec<String> = active_subs
                                    .iter()
                                    .filter(|inst| !current_subscriptions.contains(*inst))
                                    .cloned()
                                    .collect();
                                
                                // Subscribe to new instruments
                                if !to_subscribe.is_empty() {
                                    info!("Synchronizing subscriptions for broker {:?}: subscribing to {} instruments", 
                                        broker_id_clone, to_subscribe.len());
                                    md_actor_clone.do_send(Subscribe {
                                        id: Uuid::new_v4(),
                                        instruments: to_subscribe,
                                    });
                                }
                                
                                // Find instruments that need to be unsubscribed
                                let to_unsubscribe: Vec<String> = current_subscriptions
                                    .iter()
                                    .filter(|inst| !active_subs.contains(*inst))
                                    .cloned()
                                    .collect();
                                
                                // Unsubscribe from old instruments
                                if !to_unsubscribe.is_empty() {
                                    info!("Synchronizing subscriptions for broker {:?}: unsubscribing from {} instruments", 
                                        broker_id_clone, to_unsubscribe.len());
                                    md_actor_clone.do_send(Unsubscribe {
                                        id: Uuid::new_v4(),
                                        instruments: to_unsubscribe,
                                    });
                                }
                            }))
                        });
                    }
                }
            });
        
        // Execute the future
        ctx.spawn(future);
    }

    // 添加获取分发器的方法
    pub fn get_distributor(&self) -> Addr<MarketDataDistributor> {
        self.distributor.clone()
    }
}

impl Handler<Subscribe> for MarketDataConnector {
    type Result = ();

    fn handle(&mut self, msg: Subscribe, _: &mut Self::Context) -> Self::Result {
        info!(
            "Subscribing to instruments for client {}: {:?}",
            msg.id, msg.instruments
        );
        
        // Forward subscription to all market data sources
        for (broker_id, md_actor) in &self.md_sources {
            info!("Subscribing broker {:?} to instruments", broker_id);
            md_actor.do_send(Subscribe {
                id: msg.id,
                instruments: msg.instruments.clone(),
            });
        }
        
        // Register client's subscriptions with distributor
        for instrument in &msg.instruments {
            self.distributor.do_send(AddSubscription {
                instrument: instrument.clone(),
                client_id: msg.id,
            });
        }
    }
}

impl Handler<Unsubscribe> for MarketDataConnector {
    type Result = ();

    fn handle(&mut self, msg: Unsubscribe, ctx: &mut Self::Context) -> Self::Result {
        info!(
            "Unsubscribing from instruments for client {}: {:?}",
            msg.id, msg.instruments
        );
        
        // Unregister client's subscriptions with distributor
        for instrument in &msg.instruments {
            self.distributor.do_send(RemoveSubscription {
                instrument: instrument.clone(),
                client_id: msg.id,
            });
        }
        
        // Get all market data sources
        let md_sources: Vec<((String, String), Addr<MarketDataActor>)> = self.md_sources
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
            
        // Check if any instruments no longer have subscribers
        let distributor = self.distributor.clone();
        
        // Create a future that processes all market data sources
        let future = distributor
            .send(GetAllSubscriptions {})
            .into_actor(self)
            .map(move |result, _act, _ctx| {
                if let Ok(active_subscriptions) = result {
                    for (broker_id, md_actor) in md_sources {
                        // Create a separate future for each market data source with its own copy of active_subscriptions
                        let active_subs = active_subscriptions.clone();
                        let broker_id_clone = broker_id.clone();
                        let md_actor_clone = md_actor.clone();
                        
                        // Using do_send instead of send+wait to avoid blocking
                        md_actor.do_send(GetSubscriptions { 
                            id: Uuid::nil(),
                            // Process the result in another message
                            callback: Some(Box::new(move |curr_subs| {
                                // Find instruments to unsubscribe from
                                let to_unsubscribe: Vec<String> = curr_subs
                                    .into_iter()
                                    .filter(|inst| !active_subs.contains(inst))
                                    .collect();
                                
                                if !to_unsubscribe.is_empty() {
                                    info!(
                                        "Unsubscribing broker {:?} from unused instruments: {:?}",
                                        broker_id_clone, to_unsubscribe
                                    );
                                    
                                    md_actor_clone.do_send(Unsubscribe {
                                        id: Uuid::nil(),
                                        instruments: to_unsubscribe,
                                    });
                                }
                            }))
                        });
                    }
                }
            });
            
        ctx.spawn(future);
    }
}

impl Handler<GetSubscriptions> for MarketDataConnector {
    type Result = ResponseFuture<Vec<String>>;

    fn handle(&mut self, msg: GetSubscriptions, _: &mut Self::Context) -> Self::Result {
        // Get unique subscriptions from all market data sources
        // For simplicity, we'll just use the first market data source
        if let Some((_, md_actor)) = self.md_sources.iter().next() {
            let fut = md_actor.send(GetSubscriptions { 
                id: msg.id,
                callback: None  // We're using the future result instead
            });
            
            Box::pin(async move {
                match fut.await {
                    Ok(subscriptions) => subscriptions,
                    Err(e) => {
                        error!("Failed to get subscriptions: {}", e);
                        Vec::new()
                    }
                }
            })
        } else {
            Box::pin(async { Vec::new() })
        }
    }
}

// 直接处理 WebSocketConnect 而不是 Connect
impl Handler<WebSocketConnect> for MarketDataConnector {
    type Result = ();

    fn handle(&mut self, msg: WebSocketConnect, _: &mut Self::Context) -> Self::Result {
        let client_id = msg.id;
        
        // 将 WebSocket 客户端直接注册到 distributor
        self.distributor.do_send(RegisterDataReceiver {
            client_id: client_id.to_string(),
            addr: msg.addr,
            instruments: Vec::new(),
        });
        
        info!("Client {} connected and registered with distributor", client_id);
    }
}

// 直接处理 WebSocketDisconnect 而不是 Disconnect
impl Handler<WebSocketDisconnect> for MarketDataConnector {
    type Result = ();

    fn handle(&mut self, msg: WebSocketDisconnect, _: &mut Self::Context) -> Self::Result {
        let client_id = msg.id;
        // 从 clients 映射中移除客户端
        self.clients.remove(&client_id);
        
        // 从 distributor 中注销
        self.distributor.do_send(UnregisterDataReceiver {
            client_id: client_id.to_string(),
        });
        
        info!("Client {} disconnected from distributor", client_id);
    }
}

impl Handler<StopMarketData> for MarketDataConnector {
    type Result = ();

    fn handle(&mut self, _: StopMarketData, _: &mut Self::Context) -> Self::Result {
        info!("Stopping all market data sources");
        
        // Stop all market data sources
        for (broker_id, md_actor) in &self.md_sources {
            info!("Stopping market data for broker {:?}", broker_id);
            md_actor.do_send(StopMarketData);
        }
        
    }
}

// Handle MarketDataUpdates from the MarketDataActor and forward to clients
impl Handler<MarketDataUpdate> for MarketDataConnector {
    type Result = ();

    fn handle(&mut self, msg: MarketDataUpdate, _: &mut Self::Context) -> Self::Result {
        // Forward to distributor only
        self.distributor.do_send(msg.clone());
        
        // 不再直接发送给客户端，而是通过distributor发送
    }
}

