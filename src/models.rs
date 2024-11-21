use std::default;

use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;

#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy, Type, PartialEq)]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    #[default]
    User,
}

impl UserRole {
    pub fn to_str(&self) -> &str {
        match self {
            UserRole::Admin => "admin",
            UserRole::User => "user",
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, FromRow, Type, Clone)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub verified: bool,
    pub password: String,
    pub verification_token: Option<String>,
    pub token_expires_at: Option<DateTime<Utc>>,
    pub role: UserRole,
    #[serde(rename = "createdAt")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<DateTime<Utc>>,
}
