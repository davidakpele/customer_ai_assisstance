// use kalosm::{language::{Llama, LlamaSource},
//  source::FileSource};
// use std::sync::Arc;
// use once_cell::sync::OnceCell;
// use anyhow::Result;

// use crate::services::llm_service::{LlmService};

// pub static LLM_SERVICE: OnceCell<Arc<LlmService>> = OnceCell::new();

// pub async fn init_llm_service() -> Result<()> {
//     println!("Loading LLM model from file...");

//    let llama = Llama::builder()
//     .with_source(
//         LlamaSource::new() // or some specific source constructor
//             .with_model(vec![FileSource::new("../../../ai_model/wiki-news-300d-1M-subword.bin")])
//     )
//     .build()
//     .await?;

//     println!("Model loaded successfully!");

//     let service = Arc::new(LlmService::new(Arc::new(llama)));
//     LLM_SERVICE
//         .set(service)
//         .map_err(|_| anyhow::anyhow!("LLM service already initialized"))?;

//     Ok(())
// }
