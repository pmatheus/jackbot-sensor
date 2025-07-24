use axum::{
    async_trait,
    extract::{FromRequestParts, Request},
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    RequestPartsExt,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, warn};

/// JWT Claims structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,        // Subject (user ID)
    pub user_id: String,    // User ID
    pub email: String,      // Email
    pub exp: i64,           // Expiration time
    pub iat: i64,           // Issued at
    pub iss: String,        // Issuer
    pub aud: String,        // Audience
}

/// Authenticated user information
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub email: String,
}

/// JWT Validator for secure authentication
#[derive(Clone)]
pub struct JwtValidator {
    decoding_key: DecodingKey,
    validation: Validation,
}

impl JwtValidator {
    /// Create a new JWT validator with the given secret
    pub fn new(secret: &str) -> Self {
        let mut validation = Validation::default();
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.leeway = 60; // 60 seconds leeway for clock skew
        
        // Set expected issuer and audience
        validation.set_issuer(&["https://securetoken.google.com/jackbot-sensor"]);
        validation.set_audience(&["jackbot-sensor"]);
        
        Self {
            decoding_key: DecodingKey::from_secret(secret.as_ref()),
            validation,
        }
    }
    
    /// Validate a JWT token and return the user information
    pub fn validate_token(&self, token: &str) -> Result<AuthUser, AuthError> {
        let token_data = decode::<Claims>(token, &self.decoding_key, &self.validation)
            .map_err(|e| {
                warn!("JWT validation failed: {}", e);
                match e.kind() {
                    jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                    jsonwebtoken::errors::ErrorKind::InvalidToken => AuthError::InvalidToken,
                    jsonwebtoken::errors::ErrorKind::InvalidSignature => AuthError::InvalidSignature,
                    _ => AuthError::InvalidToken,
                }
            })?;
        
        Ok(AuthUser {
            user_id: token_data.claims.user_id,
            email: token_data.claims.email,
        })
    }
}

/// Authentication errors
#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken,
    InvalidSignature,
    TokenExpired,
    Unauthorized,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing authentication token"),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid authentication token"),
            AuthError::InvalidSignature => (StatusCode::UNAUTHORIZED, "Invalid token signature"),
            AuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "Token expired"),
            AuthError::Unauthorized => (StatusCode::FORBIDDEN, "Unauthorized access"),
        };
        
        (status, message).into_response()
    }
}

/// Axum extractor for authenticated requests
#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AuthError;
    
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Extract the JWT validator from request extensions
        let validator = parts
            .extensions
            .get::<Arc<JwtValidator>>()
            .ok_or(AuthError::Unauthorized)?;
        
        // Extract the Authorization header
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(AuthError::MissingToken)?;
        
        // Check for Bearer token
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(AuthError::InvalidToken)?;
        
        // Validate the token
        validator.validate_token(token)
    }
}

/// Middleware for protecting routes
pub async fn auth_middleware(
    mut req: Request,
    next: axum::middleware::Next,
) -> Result<Response, AuthError> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(AuthError::MissingToken)?;
    
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AuthError::InvalidToken)?;
    
    // Get validator from extensions
    let validator = req
        .extensions()
        .get::<Arc<JwtValidator>>()
        .ok_or(AuthError::Unauthorized)?
        .clone();
    
    // Validate token
    let auth_user = validator.validate_token(token)?;
    
    // Insert authenticated user into request extensions
    req.extensions_mut().insert(auth_user);
    
    Ok(next.run(req).await)
}

/// Extract user ID from validated JWT claims
pub fn extract_user_id_secure(auth_user: &AuthUser) -> String {
    auth_user.user_id.clone()
}