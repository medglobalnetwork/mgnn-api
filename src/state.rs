use std::sync::Arc;
use reqwest::Client;
use crate::config::Config;

/// Shared application state passed to all handlers via Axum's State extractor.
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Config,
    pub http: Client,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Build the standard Supabase REST headers (apikey + Authorization).
    pub fn supabase_headers(&self) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("apikey", self.config.supabase_key.parse().unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("")));
        h.insert(
            "Authorization",
            format!("Bearer {}", self.config.supabase_key).parse().unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("")),
        );
        h.insert("Content-Type", "application/json".parse().unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("application/json")));
        h
    }

    /// REST base URL for Supabase.
    pub fn rest_url(&self) -> String {
        format!("{}/rest/v1", self.config.supabase_url)
    }
}

/// Convenience type alias for Axum state injection.
pub type SharedState = Arc<AppState>;
