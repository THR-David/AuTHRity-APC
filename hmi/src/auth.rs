use std::sync::Arc;
use std::str::FromStr;

use argon2::{
    password_hash::{rand_core::{OsRng, RngCore}, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::FromRef,
    extract::{FromRequestParts, State},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, QueryBuilder, Row, Sqlite, SqlitePool};
use tower_sessions::Session;
use chrono::{DateTime, Duration, Utc};

use crate::state::AppState;

#[derive(Clone)]
pub struct AuthService {
    pool: SqlitePool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Viewer,
    Operator,
    Engineer,
    Admin,
}

impl Role {
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "viewer" => Some(Self::Viewer),
            "operator" => Some(Self::Operator),
            "engineer" => Some(Self::Engineer),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Operator => "operator",
            Self::Engineer => "engineer",
            Self::Admin => "admin",
        }
    }

    pub fn allows(self, permission: Permission) -> bool {
        match self {
            Self::Viewer => matches!(permission, Permission::ReadView),
            Self::Operator => matches!(
                permission,
                Permission::ReadView
                    | Permission::RuntimeWriteBasic
                    | Permission::RuntimeWriteLimitsOperational
            ),
            Self::Engineer => matches!(
                permission,
                Permission::ReadView
                    | Permission::RuntimeWriteBasic
                    | Permission::RuntimeWriteLimitsOperational
                    | Permission::RuntimeWriteLimitsSafety
                    | Permission::ModelDeploy
                    | Permission::ControllerLifecycle
            ),
            Self::Admin => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    ReadView,
    RuntimeWriteBasic,
    RuntimeWriteLimitsOperational,
    RuntimeWriteLimitsSafety,
    ModelDeploy,
    ControllerLifecycle,
    InfraWrite,
    UserManage,
    AuditRead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeWriteKind {
    Basic,
    OperationalLimit,
    SafetyLimit,
}

pub fn classify_runtime_write(node_id: &str) -> RuntimeWriteKind {
    let lowered = node_id.to_ascii_lowercase();
    if lowered.contains("lowlow")
        || lowered.contains("highhigh")
        || lowered.contains("lo_lo")
        || lowered.contains("hi_hi")
    {
        return RuntimeWriteKind::SafetyLimit;
    }

    if lowered.contains("low") || lowered.contains("high") {
        return RuntimeWriteKind::OperationalLimit;
    }

    RuntimeWriteKind::Basic
}

pub fn required_runtime_write_permission(node_id: &str) -> Permission {
    match classify_runtime_write(node_id) {
        RuntimeWriteKind::SafetyLimit => Permission::RuntimeWriteLimitsSafety,
        RuntimeWriteKind::OperationalLimit => Permission::RuntimeWriteLimitsOperational,
        RuntimeWriteKind::Basic => Permission::RuntimeWriteBasic,
    }
}

pub fn permission_audit_label(permission: Permission) -> &'static str {
    match permission {
        Permission::RuntimeWriteBasic => "runtime:write_basic",
        Permission::RuntimeWriteLimitsOperational => "runtime:write_limits_operational",
        Permission::RuntimeWriteLimitsSafety => "runtime:write_limits_safety",
        Permission::ReadView => "read:view",
        Permission::ModelDeploy => "model:deploy",
        Permission::ControllerLifecycle => "controller:lifecycle",
        Permission::InfraWrite => "infra:write",
        Permission::UserManage => "user:manage",
        Permission::AuditRead => "audit:read",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
    pub role: Role,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManagedUser {
    pub id: i64,
    pub username: String,
    pub role: Role,
    pub disabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub id: i64,
    pub actor_username: Option<String>,
    pub actor_role: Option<String>,
    pub action: String,
    pub target: Option<String>,
    pub result: String,
    pub detail: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct UserAuthState {
    pub id: i64,
    pub username: String,
    pub role: Role,
    pub disabled: bool,
    pub force_password_change: bool,
    pub auth_version: i64,
}

impl CurrentUser {
    pub fn require(&self, permission: Permission) -> Result<(), AuthError> {
        if self.role.allows(permission) {
            Ok(())
        } else {
            Err(AuthError::Forbidden)
        }
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
    Arc<AuthService>: FromRef<S>,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthError::Unauthorized)?;

        let id = session
            .get::<i64>("user_id")
            .await
            .map_err(|_| AuthError::Unauthorized)?
            .ok_or(AuthError::Unauthorized)?;

        let username = session
            .get::<String>("username")
            .await
            .map_err(|_| AuthError::Unauthorized)?
            .ok_or(AuthError::Unauthorized)?;

        let role_raw = session
            .get::<String>("role")
            .await
            .map_err(|_| AuthError::Unauthorized)?
            .ok_or(AuthError::Unauthorized)?;

        let session_auth_version = session
            .get::<i64>("auth_version")
            .await
            .map_err(|_| AuthError::Unauthorized)?
            .ok_or(AuthError::Unauthorized)?;

        let role = Role::from_str(&role_raw).ok_or(AuthError::Unauthorized)?;

        let auth = Arc::<AuthService>::from_ref(state);
        let current = auth
            .get_user_auth_state(id)
            .await?
            .ok_or(AuthError::Unauthorized)?;

        if current.disabled {
            return Err(AuthError::Unauthorized);
        }

        if current.auth_version != session_auth_version {
            return Err(AuthError::Unauthorized);
        }

        if current.username != username || current.role != role {
            return Err(AuthError::Unauthorized);
        }

        Ok(Self { id, username, role })
    }
}

#[derive(Debug)]
pub enum AuthError {
    Unauthorized,
    Forbidden,
    InvalidRequest(&'static str),
    Internal,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AuthError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized"),
            AuthError::Forbidden => (StatusCode::FORBIDDEN, "Forbidden"),
            AuthError::InvalidRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AuthError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
        };

        (status, msg).into_response()
    }
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub username: String,
    pub role: Role,
    pub csrf_token: String,
    pub force_password_change: bool,
}

#[derive(Serialize)]
pub struct MeResponse {
    pub authenticated: bool,
    pub username: Option<String>,
    pub role: Option<Role>,
    pub csrf_token: Option<String>,
    pub force_password_change: bool,
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone)]
pub struct CsrfVerified;

#[axum::async_trait]
impl<S> FromRequestParts<S> for CsrfVerified
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthError::Unauthorized)?;

        let expected = session
            .get::<String>("csrf_token")
            .await
            .map_err(|_| AuthError::Unauthorized)?
            .ok_or(AuthError::Unauthorized)?;

        let provided = parts
            .headers
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok())
            .ok_or(AuthError::Forbidden)?;

        if provided != expected {
            return Err(AuthError::Forbidden);
        }

        Ok(Self)
    }
}

impl AuthService {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let normalized_url = normalize_sqlite_url(database_url);
        let options = SqliteConnectOptions::from_str(&normalized_url)?.create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await?;
        let service = Self { pool };
        service.init_schema().await?;
        Ok(service)
    }

    async fn init_schema(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                role TEXT NOT NULL,
                force_password_change INTEGER NOT NULL DEFAULT 0,
                disabled INTEGER NOT NULL DEFAULT 0,
                failed_attempts INTEGER NOT NULL DEFAULT 0,
                lockout_until TEXT,
                auth_version INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        let _ = sqlx::query("ALTER TABLE users ADD COLUMN failed_attempts INTEGER NOT NULL DEFAULT 0")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE users ADD COLUMN lockout_until TEXT")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE users ADD COLUMN auth_version INTEGER NOT NULL DEFAULT 0")
            .execute(&self.pool)
            .await;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS audit_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                actor_user_id INTEGER,
                actor_username TEXT,
                actor_role TEXT,
                action TEXT NOT NULL,
                target TEXT,
                result TEXT NOT NULL,
                detail TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn ensure_bootstrap_admin(
        &self,
        username: &str,
        password: &str,
    ) -> Result<(), AuthError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(|_| AuthError::Internal)?;

        if count > 0 {
            return Ok(());
        }

        let hash = hash_password(password).map_err(|_| AuthError::Internal)?;
        sqlx::query(
            "INSERT INTO users (username, password_hash, role, force_password_change) VALUES (?, ?, ?, 1)",
        )
        .bind(username)
        .bind(hash)
        .bind(Role::Admin.as_str())
        .execute(&self.pool)
        .await
        .map_err(|_| AuthError::Internal)?;

        self.audit(
            None,
            "auth.bootstrap_admin",
            Some(username),
            "success",
            Some("Created initial admin user"),
        )
        .await;

        Ok(())
    }

    pub async fn verify_credentials(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<CurrentUser>, AuthError> {
        let row = sqlx::query(
            "SELECT id, username, password_hash, role, disabled, force_password_change, failed_attempts, lockout_until FROM users WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| AuthError::Internal)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let disabled: i64 = row.try_get("disabled").map_err(|_| AuthError::Internal)?;
        if disabled == 1 {
            return Ok(None);
        }

        let lockout_until: Option<String> = row.try_get("lockout_until").map_err(|_| AuthError::Internal)?;
        if let Some(lockout) = lockout_until {
            if let Ok(ts) = DateTime::parse_from_rfc3339(&lockout) {
                if ts.with_timezone(&Utc) > Utc::now() {
                    return Ok(None);
                }
            }
        }

        let hash: String = row
            .try_get("password_hash")
            .map_err(|_| AuthError::Internal)?;

        let parsed_hash = PasswordHash::new(&hash).map_err(|_| AuthError::Internal)?;
        if Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_err()
        {
            let failed_attempts: i64 = row.try_get("failed_attempts").unwrap_or(0);
            let next_attempts = failed_attempts + 1;
            let lockout_until = if next_attempts >= 5 {
                Some((Utc::now() + Duration::minutes(15)).to_rfc3339())
            } else {
                None
            };

            let _ = sqlx::query("UPDATE users SET failed_attempts = ?, lockout_until = ? WHERE username = ?")
                .bind(next_attempts)
                .bind(lockout_until)
                .bind(username)
                .execute(&self.pool)
                .await;

            return Ok(None);
        }

        let _ = sqlx::query("UPDATE users SET failed_attempts = 0, lockout_until = NULL WHERE username = ?")
            .bind(username)
            .execute(&self.pool)
            .await;

        let role_raw: String = row.try_get("role").map_err(|_| AuthError::Internal)?;
        let role = Role::from_str(&role_raw).ok_or(AuthError::Internal)?;

        Ok(Some(CurrentUser {
            id: row.try_get("id").map_err(|_| AuthError::Internal)?,
            username: row.try_get("username").map_err(|_| AuthError::Internal)?,
            role,
        }))
    }

    pub async fn get_user_auth_state(&self, user_id: i64) -> Result<Option<UserAuthState>, AuthError> {
        let row = sqlx::query(
            "SELECT id, username, role, disabled, force_password_change, auth_version FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| AuthError::Internal)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let role_raw: String = row.try_get("role").map_err(|_| AuthError::Internal)?;
        let role = Role::from_str(&role_raw).ok_or(AuthError::Internal)?;
        let disabled: i64 = row.try_get("disabled").map_err(|_| AuthError::Internal)?;
        let force_password_change: i64 = row.try_get("force_password_change").map_err(|_| AuthError::Internal)?;

        Ok(Some(UserAuthState {
            id: row.try_get("id").map_err(|_| AuthError::Internal)?,
            username: row.try_get("username").map_err(|_| AuthError::Internal)?,
            role,
            disabled: disabled == 1,
            force_password_change: force_password_change == 1,
            auth_version: row.try_get("auth_version").map_err(|_| AuthError::Internal)?,
        }))
    }

    pub async fn verify_current_password(&self, user_id: i64, password: &str) -> Result<bool, AuthError> {
        let row = sqlx::query("SELECT password_hash FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| AuthError::Internal)?;

        let Some(row) = row else { return Ok(false); };
        let hash: String = row.try_get("password_hash").map_err(|_| AuthError::Internal)?;
        let parsed_hash = PasswordHash::new(&hash).map_err(|_| AuthError::Internal)?;
        Ok(Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok())
    }

    pub async fn change_password(&self, user_id: i64, new_password: &str) -> Result<(), AuthError> {
        if new_password.is_empty() {
            return Err(AuthError::InvalidRequest("New password is required"));
        }

        let hash = hash_password(new_password)?;
        let result = sqlx::query(
            "UPDATE users SET password_hash = ?, force_password_change = 0, failed_attempts = 0, lockout_until = NULL, auth_version = auth_version + 1 WHERE id = ?",
        )
        .bind(hash)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|_| AuthError::Internal)?;

        if result.rows_affected() == 0 {
            return Err(AuthError::InvalidRequest("User not found"));
        }

        Ok(())
    }

    pub async fn get_force_password_change(&self, user_id: i64) -> Result<bool, AuthError> {
        let value: Option<i64> = sqlx::query_scalar("SELECT force_password_change FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| AuthError::Internal)?;

        Ok(value.unwrap_or(0) == 1)
    }

    pub async fn list_users(&self) -> Result<Vec<ManagedUser>, AuthError> {
        let rows = sqlx::query("SELECT id, username, role, disabled, created_at FROM users ORDER BY id ASC")
            .fetch_all(&self.pool)
            .await
            .map_err(|_| AuthError::Internal)?;

        let mut users = Vec::with_capacity(rows.len());
        for row in rows {
            let role_raw: String = row.try_get("role").map_err(|_| AuthError::Internal)?;
            let role = Role::from_str(&role_raw).ok_or(AuthError::Internal)?;
            let disabled: i64 = row.try_get("disabled").map_err(|_| AuthError::Internal)?;
            users.push(ManagedUser {
                id: row.try_get("id").map_err(|_| AuthError::Internal)?,
                username: row.try_get("username").map_err(|_| AuthError::Internal)?,
                role,
                disabled: disabled == 1,
                created_at: row.try_get("created_at").map_err(|_| AuthError::Internal)?,
            });
        }

        Ok(users)
    }

    pub async fn create_user(&self, username: &str, password: &str, role: Role) -> Result<(), AuthError> {
        if username.trim().is_empty() || password.is_empty() {
            return Err(AuthError::InvalidRequest("Username and password are required"));
        }

        let hash = hash_password(password)?;
        let result = sqlx::query(
            "INSERT INTO users (username, password_hash, role, force_password_change) VALUES (?, ?, ?, 1)",
        )
        .bind(username.trim())
        .bind(hash)
        .bind(role.as_str())
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("UNIQUE constraint failed") {
                    Err(AuthError::InvalidRequest("Username already exists"))
                } else {
                    Err(AuthError::Internal)
                }
            }
        }
    }

    pub async fn update_user_role(&self, user_id: i64, role: Role) -> Result<(), AuthError> {
        let result = sqlx::query("UPDATE users SET role = ?, auth_version = auth_version + 1 WHERE id = ?")
            .bind(role.as_str())
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|_| AuthError::Internal)?;

        if result.rows_affected() == 0 {
            return Err(AuthError::InvalidRequest("User not found"));
        }

        Ok(())
    }

    pub async fn set_user_disabled(&self, user_id: i64, disabled: bool) -> Result<(), AuthError> {
        let result = sqlx::query("UPDATE users SET disabled = ?, auth_version = auth_version + 1 WHERE id = ?")
            .bind(if disabled { 1 } else { 0 })
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|_| AuthError::Internal)?;

        if result.rows_affected() == 0 {
            return Err(AuthError::InvalidRequest("User not found"));
        }

        Ok(())
    }

    pub async fn reset_user_password(&self, user_id: i64, new_password: &str) -> Result<(), AuthError> {
        if new_password.is_empty() {
            return Err(AuthError::InvalidRequest("New password is required"));
        }

        let hash = hash_password(new_password)?;
        let result = sqlx::query(
            "UPDATE users SET password_hash = ?, force_password_change = 1, auth_version = auth_version + 1 WHERE id = ?",
        )
        .bind(hash)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|_| AuthError::Internal)?;

        if result.rows_affected() == 0 {
            return Err(AuthError::InvalidRequest("User not found"));
        }

        Ok(())
    }

    pub async fn list_audit_events(
        &self,
        limit: i64,
        action: Option<&str>,
        result: Option<&str>,
        actor: Option<&str>,
    ) -> Result<Vec<AuditEvent>, AuthError> {
        let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT id, actor_username, actor_role, action, target, result, detail, created_at FROM audit_events WHERE 1=1",
        );

        if let Some(action_filter) = action {
            query_builder.push(" AND action LIKE ");
            query_builder.push_bind(format!("%{}%", action_filter));
        }

        if let Some(result_filter) = result {
            query_builder.push(" AND result = ");
            query_builder.push_bind(result_filter);
        }

        if let Some(actor_filter) = actor {
            query_builder.push(" AND actor_username LIKE ");
            query_builder.push_bind(format!("%{}%", actor_filter));
        }

        query_builder.push(" ORDER BY id DESC LIMIT ");
        query_builder.push_bind(limit.max(1).min(1000));

        let rows = query_builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|_| AuthError::Internal)?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            events.push(AuditEvent {
                id: row.try_get("id").map_err(|_| AuthError::Internal)?,
                actor_username: row.try_get("actor_username").map_err(|_| AuthError::Internal)?,
                actor_role: row.try_get("actor_role").map_err(|_| AuthError::Internal)?,
                action: row.try_get("action").map_err(|_| AuthError::Internal)?,
                target: row.try_get("target").map_err(|_| AuthError::Internal)?,
                result: row.try_get("result").map_err(|_| AuthError::Internal)?,
                detail: row.try_get("detail").map_err(|_| AuthError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| AuthError::Internal)?,
            });
        }

        Ok(events)
    }

    pub async fn audit(
        &self,
        actor: Option<&CurrentUser>,
        action: &str,
        target: Option<&str>,
        result: &str,
        detail: Option<&str>,
    ) {
        let _ = sqlx::query(
            "INSERT INTO audit_events (actor_user_id, actor_username, actor_role, action, target, result, detail) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(actor.map(|a| a.id))
        .bind(actor.map(|a| a.username.as_str()))
        .bind(actor.map(|a| a.role.as_str()))
        .bind(action)
        .bind(target)
        .bind(result)
        .bind(detail)
        .execute(&self.pool)
        .await;
    }
}

fn normalize_sqlite_url(input: &str) -> String {
    if input.starts_with("sqlite://") && !input.starts_with("sqlite:///") {
        let rest = input.trim_start_matches("sqlite://");
        return format!("sqlite:{}", rest);
    }
    input.to_string()
}

fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| AuthError::Internal)?
        .to_string();
    Ok(hash)
}

fn generate_csrf_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub async fn login(
    State(state): State<AppState>,
    session: Session,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, AuthError> {
    if payload.username.trim().is_empty() || payload.password.is_empty() {
        return Err(AuthError::InvalidRequest("Username and password are required"));
    }

    let user = state
        .auth
        .verify_credentials(payload.username.trim(), &payload.password)
        .await?;

    let Some(user) = user else {
        state
            .auth
            .audit(None, "auth.login", Some(payload.username.trim()), "denied", None)
            .await;
        return Err(AuthError::Unauthorized);
    };

    session
        .insert("user_id", user.id)
        .await
        .map_err(|_| AuthError::Internal)?;
    session
        .insert("username", user.username.clone())
        .await
        .map_err(|_| AuthError::Internal)?;
    session
        .insert("role", user.role.as_str().to_string())
        .await
        .map_err(|_| AuthError::Internal)?;

    let auth_state = state
        .auth
        .get_user_auth_state(user.id)
        .await?
        .ok_or(AuthError::Unauthorized)?;
    session
        .insert("auth_version", auth_state.auth_version)
        .await
        .map_err(|_| AuthError::Internal)?;

    let csrf_token = generate_csrf_token();
    session
        .insert("csrf_token", csrf_token.clone())
        .await
        .map_err(|_| AuthError::Internal)?;

    state
        .auth
        .audit(Some(&user), "auth.login", None, "success", None)
        .await;

    let force_password_change = state
        .auth
        .get_force_password_change(user.id)
        .await
        .unwrap_or(false);

    Ok(Json(LoginResponse {
        username: user.username,
        role: user.role,
        csrf_token,
        force_password_change,
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    _: CsrfVerified,
    session: Session,
    user: CurrentUser,
) -> Result<impl IntoResponse, AuthError> {
    session.delete().await.map_err(|_| AuthError::Internal)?;
    state
        .auth
        .audit(Some(&user), "auth.logout", None, "success", None)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(
    State(state): State<AppState>,
    session: Session,
    user: Option<CurrentUser>,
) -> impl IntoResponse {
    if let Some(current) = user {
        let csrf_token = session.get::<String>("csrf_token").await.ok().flatten();
        let force_password_change = state
            .auth
            .get_force_password_change(current.id)
            .await
            .unwrap_or(false);
        Json(MeResponse {
            authenticated: true,
            username: Some(current.username),
            role: Some(current.role),
            csrf_token,
            force_password_change,
        })
    } else {
        Json(MeResponse {
            authenticated: false,
            username: None,
            role: None,
            csrf_token: None,
            force_password_change: false,
        })
    }
}

pub async fn change_password(
    State(state): State<AppState>,
    _: CsrfVerified,
    user: CurrentUser,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<impl IntoResponse, AuthError> {
    if payload.new_password.trim().len() < 8 {
        return Err(AuthError::InvalidRequest("New password must be at least 8 characters"));
    }

    let force_required = state
        .auth
        .get_force_password_change(user.id)
        .await?;

    if !force_required {
        let valid = state
            .auth
            .verify_current_password(user.id, &payload.current_password)
            .await?;
        if !valid {
            state
                .auth
                .audit(Some(&user), "auth.change_password", None, "denied", Some("invalid current password"))
                .await;
            return Err(AuthError::InvalidRequest("Current password is incorrect"));
        }
    }

    state.auth.change_password(user.id, payload.new_password.trim()).await?;
    state
        .auth
        .audit(Some(&user), "auth.change_password", None, "success", None)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub fn shared(auth: AuthService) -> Arc<AuthService> {
    Arc::new(auth)
}

#[cfg(test)]
mod tests {
    use super::{classify_runtime_write, required_runtime_write_permission, Permission, Role, RuntimeWriteKind};

    fn can_write_number(role: Role, node_id: &str) -> bool {
        let permission = required_runtime_write_permission(node_id);
        role.allows(permission)
    }

    #[test]
    fn runtime_write_classification_matches_limit_types() {
        assert_eq!(classify_runtime_write("Reactor:SP"), RuntimeWriteKind::Basic);
        assert_eq!(classify_runtime_write("Reactor:ModeTarget"), RuntimeWriteKind::Basic);
        assert_eq!(classify_runtime_write("Reactor:LowLimit"), RuntimeWriteKind::OperationalLimit);
        assert_eq!(classify_runtime_write("Reactor:HighLimit"), RuntimeWriteKind::OperationalLimit);
        assert_eq!(classify_runtime_write("Reactor:LowLowLimit"), RuntimeWriteKind::SafetyLimit);
        assert_eq!(classify_runtime_write("Reactor:HighHighLimit"), RuntimeWriteKind::SafetyLimit);
    }

    #[test]
    fn runtime_write_permission_mapping_is_correct() {
        assert_eq!(required_runtime_write_permission("Unit:SP"), Permission::RuntimeWriteBasic);
        assert_eq!(required_runtime_write_permission("Unit:LowLimit"), Permission::RuntimeWriteLimitsOperational);
        assert_eq!(required_runtime_write_permission("Unit:HighHighLimit"), Permission::RuntimeWriteLimitsSafety);
    }

    #[test]
    fn viewer_cannot_write_setpoints_or_limits() {
        assert!(!can_write_number(Role::Viewer, "Loop:SP"));
        assert!(!can_write_number(Role::Viewer, "Loop:LowLimit"));
        assert!(!can_write_number(Role::Viewer, "Loop:HighHighLimit"));
    }

    #[test]
    fn operator_can_write_setpoints_and_operational_limits_but_not_safety_limits() {
        assert!(can_write_number(Role::Operator, "Loop:SP"));
        assert!(can_write_number(Role::Operator, "Loop:ModeTarget"));
        assert!(can_write_number(Role::Operator, "Loop:LowLimit"));
        assert!(!can_write_number(Role::Operator, "Loop:HighHighLimit"));
    }

    #[test]
    fn engineer_and_admin_can_write_all_runtime_levels() {
        for role in [Role::Engineer, Role::Admin] {
            assert!(can_write_number(role, "Loop:SP"));
            assert!(can_write_number(role, "Loop:LowLimit"));
            assert!(can_write_number(role, "Loop:LowLowLimit"));
        }
    }
}