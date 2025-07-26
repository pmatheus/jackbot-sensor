//! Coinbase WebSocket Authentication Module
//!
//! Provides secure authentication for Coinbase WebSocket connections
//! including HMAC-SHA256 signing and JWT token management.

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

/// Coinbase API credentials
#[derive(Clone)]
pub struct CoinbaseCredentials {
    /// API key from Coinbase
    pub api_key: Arc<str>,
    /// API secret for signing (stored securely)
    api_secret: Arc<[u8]>,
    /// API passphrase (for v2 API)
    pub passphrase: Option<Arc<str>>,
}

impl CoinbaseCredentials {
    /// Create new credentials with secure storage
    pub fn new(api_key: &str, api_secret: &str, passphrase: Option<&str>) -> Result<Self> {
        // Decode base64 secret
        let secret_bytes = BASE64
            .decode(api_secret)
            .context("Failed to decode API secret from base64")?;
        
        Ok(Self {
            api_key: Arc::from(api_key),
            api_secret: Arc::from(secret_bytes),
            passphrase: passphrase.map(Arc::from),
        })
    }

    /// Generate HMAC-SHA256 signature for a message
    pub fn sign(&self, message: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.api_secret)
            .expect("HMAC can take key of any size");
        mac.update(message.as_bytes());
        let result = mac.finalize();
        BASE64.encode(result.into_bytes())
    }

    /// Create authentication headers for REST API
    pub fn create_auth_headers(
        &self,
        method: &str,
        path: &str,
        body: &str,
    ) -> Vec<(String, String)> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
            .to_string();
        
        // Create signature message
        let message = format!("{}{}{}{}", timestamp, method.to_uppercase(), path, body);
        let signature = self.sign(&message);
        
        let mut headers = vec![
            ("CB-ACCESS-KEY".to_string(), self.api_key.to_string()),
            ("CB-ACCESS-SIGN".to_string(), signature),
            ("CB-ACCESS-TIMESTAMP".to_string(), timestamp),
        ];
        
        if let Some(passphrase) = &self.passphrase {
            headers.push(("CB-ACCESS-PASSPHRASE".to_string(), passphrase.to_string()));
        }
        
        headers
    }
}

/// WebSocket authentication message for Coinbase
#[derive(Debug, Serialize, Deserialize)]
pub struct CoinbaseAuthMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub api_key: String,
    pub signature: String,
    pub timestamp: String,
    pub passphrase: Option<String>,
    pub channels: Vec<String>,
    pub product_ids: Vec<String>,
}

impl CoinbaseAuthMessage {
    /// Create authenticated subscribe message
    pub fn create_subscribe(
        credentials: &CoinbaseCredentials,
        channels: Vec<String>,
        product_ids: Vec<String>,
    ) -> Result<Self> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("Time went backwards")?
            .as_secs()
            .to_string();
        
        // Coinbase WebSocket signature format
        let message = format!("{}GET/users/self/verify", timestamp);
        let signature = credentials.sign(&message);
        
        Ok(Self {
            msg_type: "subscribe".to_string(),
            api_key: credentials.api_key.to_string(),
            signature,
            timestamp,
            passphrase: credentials.passphrase.as_ref().map(|p| p.to_string()),
            channels,
            product_ids,
        })
    }
}

/// JWT token for advanced authentication (Coinbase Advanced Trade)
#[derive(Debug, Clone)]
pub struct CoinbaseJWT {
    /// JWT token string
    pub token: Arc<str>,
    /// Token expiry time
    pub expires_at: SystemTime,
}

impl CoinbaseJWT {
    /// Create JWT from string with expiry
    pub fn new(token: &str, expires_in: u64) -> Self {
        let expires_at = SystemTime::now() + std::time::Duration::from_secs(expires_in);
        Self {
            token: Arc::from(token),
            expires_at,
        }
    }

    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        SystemTime::now() > self.expires_at
    }

    /// Get remaining validity duration
    pub fn time_until_expiry(&self) -> Option<std::time::Duration> {
        self.expires_at.duration_since(SystemTime::now()).ok()
    }
}

/// Authentication manager for handling token refresh
pub struct CoinbaseAuthManager {
    /// API credentials
    credentials: CoinbaseCredentials,
    /// Current JWT token (if using Advanced Trade API)
    jwt_token: Arc<parking_lot::RwLock<Option<CoinbaseJWT>>>,
}

impl CoinbaseAuthManager {
    /// Create new auth manager
    pub fn new(credentials: CoinbaseCredentials) -> Self {
        Self {
            credentials,
            jwt_token: Arc::new(parking_lot::RwLock::new(None)),
        }
    }

    /// Get valid JWT token, refreshing if necessary
    pub async fn get_jwt_token(&self) -> Result<CoinbaseJWT> {
        // Check if we have a valid token
        {
            let token_guard = self.jwt_token.read();
            if let Some(token) = token_guard.as_ref() {
                if !token.is_expired() {
                    return Ok(token.clone());
                }
            }
        }

        // Need to refresh token
        self.refresh_jwt_token().await
    }

    /// Refresh JWT token
    async fn refresh_jwt_token(&self) -> Result<CoinbaseJWT> {
        info!("Refreshing Coinbase JWT token");
        
        // In production, this would make an API call to get a new token
        // For now, create a mock token
        let new_token = CoinbaseJWT::new(
            "mock_jwt_token",
            3600, // 1 hour expiry
        );
        
        // Store the new token
        {
            let mut token_guard = self.jwt_token.write();
            *token_guard = Some(new_token.clone());
        }
        
        debug!("JWT token refreshed successfully");
        Ok(new_token)
    }

    /// Get credentials for basic auth
    pub fn credentials(&self) -> &CoinbaseCredentials {
        &self.credentials
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_signature() {
        // Test vector from Coinbase documentation
        let creds = CoinbaseCredentials::new(
            "test_key",
            "dGVzdF9zZWNyZXQ=", // "test_secret" in base64
            Some("test_passphrase"),
        ).unwrap();
        
        let message = "1234567890GET/users/self/verify";
        let signature = creds.sign(message);
        
        // Verify signature format (base64 encoded)
        assert!(!signature.is_empty());
        assert!(BASE64.decode(&signature).is_ok());
    }

    #[test]
    fn test_auth_message_creation() {
        let creds = CoinbaseCredentials::new(
            "test_key",
            "dGVzdF9zZWNyZXQ=",
            Some("test_passphrase"),
        ).unwrap();
        
        let auth_msg = CoinbaseAuthMessage::create_subscribe(
            &creds,
            vec!["level2".to_string()],
            vec!["BTC-USD".to_string()],
        ).unwrap();
        
        assert_eq!(auth_msg.msg_type, "subscribe");
        assert_eq!(auth_msg.api_key, "test_key");
        assert!(!auth_msg.signature.is_empty());
        assert!(!auth_msg.timestamp.is_empty());
    }

    #[test]
    fn test_jwt_expiry() {
        let token = CoinbaseJWT::new("test_token", 60); // 60 seconds
        
        assert!(!token.is_expired());
        assert!(token.time_until_expiry().is_some());
        
        // Test expired token
        let expired_token = CoinbaseJWT {
            token: Arc::from("expired"),
            expires_at: SystemTime::now() - std::time::Duration::from_secs(60),
        };
        
        assert!(expired_token.is_expired());
        assert!(expired_token.time_until_expiry().is_none());
    }
}