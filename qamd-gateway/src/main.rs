mod actors;
mod api;
mod config;
mod converter;
mod error;
mod ws_server;

mod snapshot;
mod types;

use actix_cors::Cors;
use actix_rt;
use actix_web::{middleware, web, App, HttpServer};
use log::info;
use std::time::Instant;

use crate::actors::md_actor::MarketDataActor;
use crate::actors::md_connector::MarketDataConnector;
use crate::actors::md_distributor::MarketDataDistributor;
use crate::api::{configure_routes, AppState};
use crate::config::Config;
use crate::error::GatewayResult;

#[actix_rt::main]
async fn main() -> GatewayResult<()> {
    // Initialize logger
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    // Load configuration
    let config = Config::load()?;
    info!("Configuration loaded");

    // Create the market data distributor actor
    let md_distributor = actix::Actor::start(MarketDataDistributor::default());
    info!("Market data distributor initialized");

    // Get broker configurations
    let broker_config = config.get_broker(None)?;
    let all_broker_configs = vec![broker_config];

    // Get default subscriptions
    let default_instruments = config.subscription.default_instruments.clone();

    // Create the market data connector actor
    let md_connector = actix::Actor::start(MarketDataConnector::new(
        all_broker_configs
            .into_iter()
            .map(|bc| bc.clone())
            .collect(),
        default_instruments,
        md_distributor.clone(),
    ));
    info!("Market data connector initialized");

    // Create application state for API endpoints
    let app_state = web::Data::new(AppState {
        md_connector: md_connector.clone(),
        start_time: Instant::now(),
    });

    // Start HTTP server
    info!(
        "Starting HTTP server at {}:{}",
        config.rest_api.host, config.rest_api.port
    );

    HttpServer::new(move || {
        // Create CORS configuration
        let cors = Cors::permissive()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(middleware::Logger::default())
            .wrap(cors)
            .app_data(app_state.clone())
            .app_data(web::Data::new(md_connector.clone()))
            .app_data(web::Data::new(md_distributor.clone()))
            .service(
                // web::resource(&config.websocket.path).route(web::get().to(ws_server::ws_handler)),
                web::resource("/api/market/{source}").route(web::get().to(ws_server::ws_handler)),
            )
            .configure(configure_routes)
    })
    .bind((config.rest_api.host.clone(), config.rest_api.port))?
    .run()
    .await?;

    Ok(())
}
