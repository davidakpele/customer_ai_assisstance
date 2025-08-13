use std::path::PathBuf;

pub struct Config {
    pub model_path: PathBuf,
    pub model_arch: &'static str,
}

impl Config {
    pub fn default() -> Self {
        Self {
            model_path: PathBuf::from("open_llama_3b-f16.bin"),
            model_arch: "llama",
        }
    }
}
