use serde::Serialize;

 #[allow(dead_code)]
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum CommunicationResponse {
    #[serde(rename = "ai_response")]
    AIResponse {
        status: String,
        response: String,
    },
    #[serde(rename = "error")]
    Error {
        status: String,
        error: String,
    },
}
