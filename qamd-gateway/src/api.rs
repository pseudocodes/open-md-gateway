use actix::Addr;
use actix_web::{get, post, web, HttpResponse, Responder};
use log::error;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use uuid::Uuid;

use crate::actors::md_connector::MarketDataConnector;
use crate::actors::messages::{GetSubscriptions, Subscribe, Unsubscribe};
use serde_json::json;

/// Request for subscription management
#[derive(Deserialize)]
pub struct SubscriptionRequest {
    pub instruments: Vec<String>,
}

/// Response for subscription management
#[derive(Serialize)]
pub struct SubscriptionsResponse {
    pub instruments: Vec<String>,
}

/// Status response
#[derive(Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub uptime: u64,
    pub connected_clients: usize,
    pub active_subscriptions: usize,
}

/// Error response
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Application state
pub struct AppState {
    /// Market data connector
    pub md_connector: Addr<MarketDataConnector>,
    /// Application start time
    pub start_time: Instant,
}

/// Health check endpoint
#[get("/health")]
async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("OpenMdGateway is running")
}

/// Get all subscribed instruments
#[get("/api/subscriptions")]
async fn get_subscriptions(data: web::Data<AppState>) -> impl Responder {
    let result = data
        .md_connector
        .send(GetSubscriptions {
            id: Uuid::nil(),
            callback: None,
        })
        .await;

    match result {
        Ok(instruments) => HttpResponse::Ok().json(SubscriptionsResponse { instruments }),
        Err(e) => {
            error!("Failed to get subscriptions: {}", e);
            HttpResponse::InternalServerError().json(json!({
                "error": format!("Failed to get subscriptions: {}", e)
            }))
        }
    }
}

/// Subscribe to instruments
#[post("/api/subscriptions")]
async fn subscribe(
    data: web::Data<AppState>,
    req: web::Json<SubscriptionRequest>,
) -> impl Responder {
    // Create a unique ID for this request
    let id = Uuid::new_v4();

    // Send subscribe message to connector
    let subscribe_result = data
        .md_connector
        .send(Subscribe {
            id,
            instruments: req.instruments.clone(),
        })
        .await;

    // Check if subscribe message was delivered
    if let Err(e) = subscribe_result {
        error!("Failed to send subscribe message: {}", e);
        return HttpResponse::InternalServerError().json(json!({
            "error": format!("Failed to subscribe: {}", e)
        }));
    }

    // Get updated subscriptions
    let result = data
        .md_connector
        .send(GetSubscriptions {
            id: Uuid::nil(),
            callback: None,
        })
        .await;

    match result {
        Ok(instruments) => HttpResponse::Ok().json(SubscriptionsResponse { instruments }),
        Err(e) => {
            error!("Failed to get subscriptions after subscribe: {}", e);
            HttpResponse::InternalServerError().json(json!({
                "error": format!("Failed to get subscriptions: {}", e)
            }))
        }
    }
}

/// Unsubscribe from instruments
#[post("/api/unsubscribe")]
async fn unsubscribe(
    data: web::Data<AppState>,
    req: web::Json<SubscriptionRequest>,
) -> impl Responder {
    // Create a unique ID for this request
    let id = Uuid::new_v4();

    // Send unsubscribe message to connector
    let unsubscribe_result = data
        .md_connector
        .send(Unsubscribe {
            id,
            instruments: req.instruments.clone(),
        })
        .await;

    // Check if unsubscribe message was delivered
    if let Err(e) = unsubscribe_result {
        error!("Failed to send unsubscribe message: {}", e);
        return HttpResponse::InternalServerError().json(json!({
            "error": format!("Failed to unsubscribe: {}", e)
        }));
    }

    // Get updated subscriptions
    let result = data
        .md_connector
        .send(GetSubscriptions {
            id: Uuid::nil(),
            callback: None,
        })
        .await;

    match result {
        Ok(instruments) => HttpResponse::Ok().json(SubscriptionsResponse { instruments }),
        Err(e) => {
            error!("Failed to get subscriptions after unsubscribe: {}", e);
            HttpResponse::InternalServerError().json(json!({
                "error": format!("Failed to get subscriptions: {}", e)
            }))
        }
    }
}

/// Get gateway status
#[get("/api/status")]
async fn get_status(data: web::Data<AppState>) -> impl Responder {
    // TODO: Implement status check via actor messages
    // For now, just return basic info
    let response = StatusResponse {
        status: "connected".to_string(), // Default to connected
        uptime: data.start_time.elapsed().as_secs(),
        connected_clients: 0,    // Will be implemented later
        active_subscriptions: 0, // Will be implemented later
    };

    HttpResponse::Ok().json(response)
}

/// Configure API routes
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .service(health_check)
            .service(get_subscriptions)
            .service(subscribe)
            .service(unsubscribe)
            .service(get_status),
    );
}
