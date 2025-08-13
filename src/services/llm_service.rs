use std::{convert::Infallible, error::Error, path::PathBuf, sync::Arc};
use llm::{InferenceParameters, InferenceRequest, InferenceResponse, InferenceFeedback, Model, ModelArchitecture, TokenizerSource};
use tokio::sync::Mutex;
use crate::config::config_llm::Config;

#[derive(Clone)]
pub struct LlmService {
    model: Arc<Mutex<Box<dyn Model>>>,
}

impl LlmService {
    pub fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let cfg = Config::default();

        let arch = match cfg.model_arch.trim().to_lowercase().as_str() {
            "llama" | "llama-2" | "llama2" => ModelArchitecture::Llama,
            "gpt2" => ModelArchitecture::Gpt2,
            _ => return Err("Unsupported architecture".into()),
        };

        let model_path = PathBuf::from(cfg.model_path);
        println!("Loading model from {}...", model_path.display());

        let model = llm::load_dynamic(
            Some(arch),
            &model_path,
            TokenizerSource::Embedded,
            Default::default(),
            llm::load_progress_callback_stdout,
        )?;

        println!("Model loaded successfully.");
        Ok(Self {
            model: Arc::new(Mutex::new(model)),
        })
    }

    pub async fn run_prompt(&self, prompt: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
        let model = self.model.lock().await;
        let mut session = model.start_session(Default::default());
        let mut output = String::new();

        session.infer::<Infallible>(
            model.as_ref(),
            &mut rand::thread_rng(),
            &InferenceRequest {
                prompt: prompt.into(),
                parameters: &InferenceParameters::default(),
                play_back_previous_tokens: false,
                maximum_token_count: Some(140),
            },
            &mut Default::default(),
            |resp| {
                if let InferenceResponse::InferredToken(t) = resp {
                    output.push_str(&t);
                }
                Ok(InferenceFeedback::Continue)
            },
        )?;

        Ok(output)
    }


}
