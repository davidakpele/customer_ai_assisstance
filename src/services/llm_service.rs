// use anyhow::Result;
// use kalosm::language::Llama;
// use std::sync::Arc;

// pub type SharedLlama = Arc<Llama>;

// pub struct LlmService {
//     llama: SharedLlama,
// }

// impl LlmService {
//     pub fn new(llama: SharedLlama) -> Self {
//         Self { llama }
//     }

//     pub async fn generate_reply(&self, prompt: &str) -> Result<String> {
//         let mut stream = self.llama.stream(prompt);
//         let reply = stream.to_string().await?;
//         Ok(reply)
//     }
// }
