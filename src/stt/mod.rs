pub mod elevenlabs;
pub mod openrouter;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

pub trait Transcriber: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &str;
    fn transcribe(&self, audio_path: &Path) -> Result<String, String>;
}

type Factory = Box<dyn Fn(serde_json::Value) -> Box<dyn Transcriber> + Send + Sync>;

static REGISTRY: LazyLock<Mutex<HashMap<String, Factory>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn register(
    name: &str,
    factory: impl Fn(serde_json::Value) -> Box<dyn Transcriber> + Send + Sync + 'static,
) {
    REGISTRY
        .lock()
        .unwrap()
        .insert(name.to_string(), Box::new(factory));
}

pub fn get(name: &str, config: &serde_json::Value) -> Result<Box<dyn Transcriber>, String> {
    let registry = REGISTRY.lock().unwrap();
    let factory = registry.get(name).ok_or_else(|| {
        let available: Vec<&str> = registry.keys().map(|s| s.as_str()).collect();
        format!(
            "unknown STT provider {:?} (available: {})",
            name,
            available.join(", ")
        )
    })?;
    Ok(factory(config.clone()))
}

pub fn init_providers() {
    register("elevenlabs", |config| {
        let api_key = config
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let model = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("scribe_v1")
            .to_string();
        Box::new(elevenlabs::ElevenLabsTranscriber::new(api_key, model))
    });
    register("openrouter", |config| {
        let api_key = config
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let model = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Box::new(openrouter::OpenRouterTranscriber::new(api_key, model))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockTranscriber {
        name: String,
    }
    impl Transcriber for MockTranscriber {
        fn name(&self) -> &str {
            &self.name
        }
        fn transcribe(&self, _audio_path: &Path) -> Result<String, String> {
            Ok("mock transcript".to_string())
        }
    }

    #[test]
    fn register_and_get_provider() {
        clear_registry();
        register("mock", |_| {
            Box::new(MockTranscriber {
                name: "mock".into(),
            })
        });
        let t = get("mock", &serde_json::Value::Null).unwrap();
        assert_eq!(t.name(), "mock");
        assert_eq!(t.transcribe(Path::new("x.mp3")).unwrap(), "mock transcript");
    }

    #[test]
    fn get_unknown_provider_errors() {
        clear_registry();
        let err = match get("nonexistent", &serde_json::Value::Null) {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        };
        assert!(err.contains("unknown"));
        assert!(err.contains("nonexistent"));
    }

    fn clear_registry() {
        REGISTRY.lock().unwrap().clear();
    }
}
