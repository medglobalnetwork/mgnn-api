use std::env;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Config {
    pub supabase_url: String,
    pub supabase_key: String,
    pub jwt_secret: String,
    pub jwt_issuer: String,
    pub access_token_ttl_secs: i64,
    pub refresh_token_ttl_secs: i64,
    pub server_port: u16,
    pub cors_origins: Vec<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            supabase_url: env_var("SUPABASE_URL", "http://localhost:54321"),
            supabase_key: env_var("SUPABASE_SERVICE_ROLE_KEY", ""),
            jwt_secret: env_var("JWT_SECRET", "mgn-dev-secret-change-me"),
            jwt_issuer: env_var("JWT_ISSUER", "mgn-api"),
            access_token_ttl_secs: env_var("ACCESS_TOKEN_TTL_SECS", "900").parse().unwrap_or(900),
            refresh_token_ttl_secs: env_var("REFRESH_TOKEN_TTL_SECS", "2592000").parse().unwrap_or(2_592_000),
            server_port: env_var("PORT", "8000").parse().unwrap_or(8000),
            cors_origins: env_var("CORS_ORIGINS", "http://localhost:3000")
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
        }
    }
}

fn env_var(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| {
        tracing::warn!("{} not set, using default", key);
        default.to_string()
    })
}
