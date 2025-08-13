use std::net::SocketAddr;
use std::sync::Arc;
use futures_util::{SinkExt, StreamExt};
use redis::{AsyncCommands, Client};
use redis::aio::Connection;
use serde_json::{json, Value};
use tokio::sync::mpsc::{self};
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;

use crate::payloads::communication_request::CommunicationRequest;
use crate::payloads::communication_response::CommunicationResponse;
use crate::payloads::connection_request::ConnectionRequest;
use crate::services::llm_service::LlmService;
use crate::{
    services::user_service::UserService,
    utils::jwt::Claims,
    ws::{ws_auth::WsAuth, ws_channel::WsBroadcaster},
};

// Store user claims and session info in Redis
pub async fn cache_user_data(
    redis: &Client,
    session_id: &str,
    user_id: u64,
    claims: &Claims,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut conn: Connection = redis.get_async_connection().await?;

    // Serialize claims
    let claims_json = serde_json::to_string(claims)?;

    // Store in Redis with session-based keys
    redis::pipe()
        .atomic()
        .hset("session:data", session_id, claims_json)
        .hset("session:user", session_id, user_id.to_string())
        .expire("session:data", 3600)
        .expire("session:user", 3600)
        .query_async::<_, ()>(&mut conn)
        .await?;

    Ok(())
}

// Remove user session from Redis
pub async fn remove_session_data(
    redis: &Client,
    session_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut conn: Connection = redis.get_async_connection().await?;
    
    redis::pipe()
        .atomic()
        .hdel("session:data", session_id)
        .hdel("session:user", session_id)
        .query_async::<_, ()>(&mut conn)
        .await?;

    Ok(())
}

pub async fn handle_ws_connection(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    client_id: Uuid,
    peer: SocketAddr,
    broadcaster: Arc<WsBroadcaster>,
    redis_client: Arc<Client>,
    user_service: Arc<UserService>,
    llm_service: Arc<LlmService>,
) {
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // 1. AUTHENTICATION PHASE 
    let (user_id, claims) = match ws_receiver.next().await {
        Some(Ok(first_msg)) => {
            match WsAuth::from_first_message(&first_msg).await {
                Ok(WsAuth(claims)) => {
                    println!("[{client_id}] JWT authentication succeeded");
                    (claims.sub as u64, claims)
                }
                Err((code, msg)) => {
                    let _ = ws_sender.send(Message::Text(
                            json!({
                                "type": "error",
                                "status": "authentication_failed",
                                "error": msg,
                                "code": code.as_u16()
                            }).to_string().into() 
                        )).await;
                    return;
                }
            }
        }
        Some(Err(e)) => {
            let _ = ws_sender.send(Message::Text(
                json!({
                    "type": "error",
                    "status": "connection_error",
                    "error": format!("Failed to read message: {}", e),
                    "code": 400
                }).to_string().into()
            )).await;
            return;
        }
        None => {
            let _ = ws_sender.send(Message::Text(
                json!({
                    "type": "error",
                    "status": "no_message",
                    "error": "No initial message received",
                    "code": 400
                }).to_string().into()
            )).await;
            return;
        }
    };

    // 2. SESSION CREATION
    let session_id = Uuid::new_v4().to_string();
    
    // Cache user data in Redis
    if let Err(e) = cache_user_data(&redis_client, &session_id, user_id, &claims).await {
        let _ = ws_sender.send(Message::Text(
            json!({
                "type": "error",
                "status": "cache_error",
                "error": format!("Failed to cache user data: {}", e),
                "code": 500
            }).to_string().into()
        )).await;
        return;
    }

    // Register client
    let (tx, mut rx) = mpsc::unbounded_channel();
    broadcaster.add_client(client_id, tx).await;

    // Send session info
    let _ = ws_sender.send(Message::Text(
        serde_json::to_string(&CommunicationResponse::AIResponse {
            status: "session_created".to_string(),
            response: json!({
                "session_id": session_id,
                "user_id": user_id
            }).to_string(),
        }).unwrap().into()
    )).await;

    // 3. MAIN MESSAGE PROCESSING LOOP
   let process_task = tokio::spawn({
        let broadcaster = broadcaster.clone();
        let redis_client = redis_client.clone();
        let session_id = session_id.clone();
        async move {
            while let Some(Ok(msg)) = ws_receiver.next().await {
                match msg {
                    Message::Text(text) => {
                        // First try to parse as ConnectionRequest
                        match serde_json::from_str::<ConnectionRequest>(&text) {
                            Ok(conn_req) => {
                                match conn_req {
                                    ConnectionRequest::Disconnect { session_id: req_session_id, user_id: _ } => {
                                        if req_session_id == session_id {
                                            // Clean up and disconnect
                                            let _ = remove_session_data(&redis_client, &session_id).await;
                                            let _ = broadcaster.send_to(
                                                &client_id,
                                                serde_json::to_string(&CommunicationResponse::AIResponse {
                                                    status: "disconnected".to_string(),
                                                    response: "Successfully disconnected".to_string(),
                                                }).unwrap()
                                            ).await;
                                            break;
                                        }
                                    }
                                    ConnectionRequest::StartConnection { .. } => {
                                        // Already authenticated, ignore new connection requests
                                        let _ = broadcaster.send_to(
                                            &client_id,
                                            serde_json::to_string(&CommunicationResponse::Error {
                                                status: "invalid_request".to_string(),
                                                error: "Already connected".to_string(),
                                            }).unwrap()
                                        ).await;
                                    }
                                }
                            }
                            Err(_) => {
                                // If not a ConnectionRequest, try to parse as CommunicationRequest
                                match serde_json::from_str::<CommunicationRequest>(&text) {
                                    Ok(comm_req) => {
                                        match comm_req {
                                            CommunicationRequest::AIRequest { prompt } => {
                                                let llm_service = llm_service.clone();
                                                let broadcaster = broadcaster.clone();
                                                let client_id = client_id.clone();

                                                tokio::spawn(async move {
                                                    match llm_service.run_prompt(&prompt).await {
                                                        Ok(ai_output) => {
                                                            let _ = broadcaster.send_to(
                                                                &client_id,
                                                                serde_json::to_string(&CommunicationResponse::AIResponse {
                                                                    status: "success".to_string(),
                                                                    response: ai_output,
                                                                }).unwrap()
                                                            ).await;
                                                        }
                                                        Err(e) => {
                                                            let _ = broadcaster.send_to(
                                                                &client_id,
                                                                serde_json::to_string(&CommunicationResponse::Error {
                                                                    status: "ai_error".to_string(),
                                                                    error: format!("AI processing failed: {}", e),
                                                                }).unwrap()
                                                            ).await;
                                                        }
                                                    }
                                                });
                                            }

                                        }
                                    }
                                    Err(_) => {
                                        // Unknown message type
                                        let _ = broadcaster.send_to(
                                            &client_id,
                                            serde_json::to_string(&CommunicationResponse::Error {
                                                status: "invalid_request".to_string(),
                                                error: "Unknown request type".to_string(),
                                            }).unwrap()
                                        ).await;
                                    }
                                }
                            }
                        }
                    }
                    Message::Close(_) => break,
                    _ => {
                        // Ignore other message types (binary, ping, pong)
                        let _ = broadcaster.send_to(
                            &client_id,
                            serde_json::to_string(&CommunicationResponse::Error {
                                status: "invalid_message".to_string(),
                                error: "Only text messages are supported".to_string(),
                            }).unwrap()
                        ).await;
                    }
                }
            }

            // Clean up on disconnect
            broadcaster.remove_client(&client_id).await;
            let _ = remove_session_data(&redis_client, &session_id).await;
        }
    });

    // Message sending task
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = process_task => (),
        _ = send_task => (),
    }

    println!("[{}] Connection closed", client_id);
}