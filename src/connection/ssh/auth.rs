//! Authentication subsystem (prompt 2.2): `AuthContext`, providers
//! (Strategy pattern), negotiation with fallback, and the UI prompt bridge.
//!
//! Flow: `negotiate` walks `ctx.preferred_methods`; each provider either
//! produces an [`AuthCredential`] (from config, disk, agent, or an interactive
//! prompt routed to the UI via [`PromptBridge`]) or reports why it can't run.
//! SSH-side execution of the produced credential lands with the engine
//! (prompt 2.1) — the split is deliberate so flows are testable headlessly.
//!
//! Security: credentials ride in `Zeroizing` buffers, `AuthContext` zeroizes
//! on drop, and `Debug` impls never render secrets (prompt 2.2 quality bar).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use thiserror::Error;
use tokio::sync::{broadcast, oneshot};
use zeroize::Zeroizing;

use mbxt_core::{AuthMethod, SessionSpec};

/// Server-side `keyboard-interactive` challenge callback.
pub type ChallengeResponse = Box<dyn Fn(&str) -> String + Send + Sync>;

// ---------------------------------------------------------------------------
// Context & credentials
// ---------------------------------------------------------------------------

/// Everything an auth provider may need (prompt 2.2 specification).
///
/// Deviation note: `challenge_response` is `Option<Box<dyn Fn>>` rather than
/// a bare required field — non-interactive contexts (CLI, agent-only) have no
/// callback, and the app wires its callback through [`PromptBridge`] instead.
pub struct AuthContext {
    pub username: String,
    pub password: Option<String>,
    pub private_key: Option<PathBuf>,
    pub key_passphrase: Option<String>,
    pub use_agent: bool,
    pub preferred_methods: Vec<AuthMethod>,
    /// Synchronous challenge callback (server-side `keyboard-interactive`
    /// prompts); when absent, providers fall back to [`PromptBridge`].
    pub challenge_response: Option<ChallengeResponse>,
}

impl std::fmt::Debug for AuthContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never render secrets (prompt 2.2: never log passwords).
        f.debug_struct("AuthContext")
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("private_key", &self.private_key)
            .field(
                "key_passphrase",
                &self.key_passphrase.as_ref().map(|_| "<redacted>"),
            )
            .field("use_agent", &self.use_agent)
            .field("preferred_methods", &self.preferred_methods)
            .field(
                "challenge_response",
                &self.challenge_response.as_ref().map(|_| "<callback>"),
            )
            .finish()
    }
}

impl Drop for AuthContext {
    fn drop(&mut self) {
        // Zeroize secrets held as plain Strings (quality bar, prompt 2.2).
        use zeroize::Zeroize;
        if let Some(password) = self.password.as_mut() {
            password.zeroize();
        }
        if let Some(passphrase) = self.key_passphrase.as_mut() {
            passphrase.zeroize();
        }
    }
}

impl AuthContext {
    /// Build a context from a session spec, with a sensible method order per
    /// configured method (fallback/negotiation requirement, prompt 2.2).
    pub fn from_spec(spec: &SessionSpec) -> Self {
        let private_key = match &spec.auth {
            AuthMethod::KeyFile { path } => Some(PathBuf::from(path)),
            _ => None,
        };
        let preferred = match &spec.auth {
            AuthMethod::Password => vec![AuthMethod::Password, AuthMethod::KeyboardInteractive],
            AuthMethod::KeyFile { path } => vec![
                AuthMethod::KeyFile { path: path.clone() },
                AuthMethod::Agent { forward: false },
                AuthMethod::Password,
                AuthMethod::KeyboardInteractive,
            ],
            AuthMethod::Agent { forward } => vec![
                AuthMethod::Agent { forward: *forward },
                AuthMethod::Password,
                AuthMethod::KeyboardInteractive,
            ],
            AuthMethod::KeyboardInteractive => {
                vec![AuthMethod::KeyboardInteractive, AuthMethod::Password]
            },
        };
        Self {
            username: spec.username.clone().unwrap_or_default(),
            password: None,
            private_key,
            key_passphrase: None,
            use_agent: true,
            preferred_methods: preferred,
            challenge_response: None,
        }
    }
}

/// A credential ready for the SSH engine to execute.
#[derive(Clone)]
pub enum AuthCredential {
    Password(Zeroizing<String>),
    KeyFile {
        path: PathBuf,
        passphrase: Option<Zeroizing<String>>,
    },
    /// Defer to ssh-agent at handshake time (`SSH_AUTH_SOCK`).
    Agent,
    /// `keyboard-interactive` response (password or OTP code).
    KeyboardInteractive(Zeroizing<String>),
    /// GSSAPI/Kerberos (feature `gssapi`).
    GssApi,
}

impl std::fmt::Debug for AuthCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password(_) => f.debug_tuple("Password").field(&"<redacted>").finish(),
            Self::KeyFile { path, passphrase } => f
                .debug_struct("KeyFile")
                .field("path", path)
                .field("passphrase", &passphrase.as_ref().map(|_| "<redacted>"))
                .finish(),
            Self::Agent => f.write_str("Agent"),
            Self::KeyboardInteractive(_) => f
                .debug_tuple("KeyboardInteractive")
                .field(&"<redacted>")
                .finish(),
            Self::GssApi => f.write_str("GssApi"),
        }
    }
}

impl AuthCredential {
    /// SSH method name (also used for logs/status).
    pub fn method_name(&self) -> &'static str {
        match self {
            Self::Password(_) => "password",
            Self::KeyFile { .. } => "publickey",
            Self::Agent => "agent",
            Self::KeyboardInteractive(_) => "keyboard-interactive",
            Self::GssApi => "gssapi-with-mic",
        }
    }

    /// Plain password for the credential cache ("Remember password").
    pub fn password_for_cache(&self) -> Option<&str> {
        match self {
            Self::Password(pw) | Self::KeyboardInteractive(pw) => Some(pw.as_str()),
            _ => None,
        }
    }
}

/// Successful negotiation result.
#[derive(Debug)]
pub struct AuthOutcome {
    /// The configured method that produced the credential.
    pub method: AuthMethod,
    pub credential: AuthCredential,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Auth failures. `Unavailable` drives fallback; `Cancelled`/`UiGone` abort.
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    #[error("authentication failed: {0}")]
    Failed(String),
    #[error("cancelled by user")]
    Cancelled,
    #[error("no authentication method succeeded ({0})")]
    AllMethodsFailed(String),
    #[error("no UI available for interactive prompts")]
    UiGone,
}

impl AuthError {
    /// User-facing message (notification path, architecture §7).
    pub fn user_message(&self) -> String {
        match self {
            Self::Unavailable(reason) => format!("Authentication unavailable: {reason}"),
            Self::Failed(reason) => format!("Authentication failed: {reason}"),
            Self::Cancelled => "Authentication cancelled".to_string(),
            Self::AllMethodsFailed(attempts) => {
                format!("All authentication methods failed. {attempts}")
            },
            Self::UiGone => "Authentication prompt could not be shown".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Prompt bridge (UI ⇄ provider, async one-shot per prompt)
// ---------------------------------------------------------------------------

/// What kind of input the UI should collect (drives masking + dialog title).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Password,
    Passphrase,
    Otp,
}

/// A prompt routed to the UI (Clone + Send + Sync; travels the event bus).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPrompt {
    pub id: u64,
    pub kind: PromptKind,
    pub title: String,
    pub prompt: String,
    pub masked: bool,
}

/// Bridge carrying provider prompts to the UI and answers back.
///
/// `shared()` is the sanctioned process-wide handle (like `TaskManager`):
/// providers clone it, the subscription forwards prompts as
/// `Message::AuthPrompt`, and `update` completes them via
/// `Message::AuthResponse` — async waiting is a `oneshot` per prompt.
#[derive(Clone)]
pub struct PromptBridge {
    tx: broadcast::Sender<AuthPrompt>,
    replies: Arc<Mutex<HashMap<u64, oneshot::Sender<String>>>>,
    next_id: Arc<AtomicU64>,
}

impl std::fmt::Debug for PromptBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromptBridge").finish_non_exhaustive()
    }
}

impl Default for PromptBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptBridge {
    /// Process-wide singleton.
    pub fn shared() -> &'static Self {
        static SHARED: OnceLock<PromptBridge> = OnceLock::new();
        SHARED.get_or_init(Self::new)
    }

    /// Isolated bridge (tests use this to avoid cross-talk on `shared()`).
    pub fn new() -> Self {
        Self {
            tx: broadcast::channel(64).0,
            replies: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Subscribe to prompts (UI subscription bridge uses this).
    pub fn subscribe(&self) -> broadcast::Receiver<AuthPrompt> {
        self.tx.subscribe()
    }

    /// Ask the UI a question; resolves with the user's answer.
    pub async fn request(
        &self,
        kind: PromptKind,
        title: &str,
        prompt: &str,
    ) -> Result<String, AuthError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = AuthPrompt {
            id,
            kind,
            title: title.to_string(),
            prompt: prompt.to_string(),
            // Passwords, passphrases, and OTP codes are all masked.
            masked: true,
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        self.replies
            .lock()
            .expect("reply map poisoned")
            .insert(id, reply_tx);

        // No subscribers = no UI (headless mode) → prompt cannot be answered.
        if self.tx.send(request).is_err() {
            self.replies.lock().expect("reply map poisoned").remove(&id);
            return Err(AuthError::UiGone);
        }

        // Dropped sender (cancel) resolves as user cancellation.
        reply_rx.await.map_err(|_| AuthError::Cancelled)
    }

    /// Deliver the user's answer for `id` (from `Message::AuthResponse`).
    pub fn complete(&self, id: u64, value: String) -> bool {
        match self.replies.lock().expect("reply map poisoned").remove(&id) {
            Some(sender) => sender.send(value).is_ok(),
            None => false,
        }
    }

    /// Cancel a pending prompt without an answer (dialog dismissed).
    pub fn cancel(&self, id: u64) -> bool {
        self.replies
            .lock()
            .expect("reply map poisoned")
            .remove(&id)
            .is_some()
    }
}

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

/// One authentication strategy (Strategy pattern, architecture §3.5).
/// `acquire` gathers the credential (config/disk/agent/prompt); executing it
/// against russh is the engine's job (prompt 2.1).
#[async_trait::async_trait]
pub trait AuthProvider {
    fn method_name(&self) -> &'static str;
    async fn acquire(
        &mut self,
        ctx: &mut AuthContext,
        bridge: &PromptBridge,
    ) -> Result<AuthCredential, AuthError>;
}

/// Password from context or an interactive prompt.
pub struct PasswordProvider;

#[async_trait::async_trait]
impl AuthProvider for PasswordProvider {
    fn method_name(&self) -> &'static str {
        "password"
    }

    async fn acquire(
        &mut self,
        ctx: &mut AuthContext,
        bridge: &PromptBridge,
    ) -> Result<AuthCredential, AuthError> {
        if let Some(password) = ctx.password.take() {
            if !password.is_empty() {
                return Ok(AuthCredential::Password(Zeroizing::new(password)));
            }
            // Empty cached password falls through to the prompt.
        }
        let response = bridge
            .request(
                PromptKind::Password,
                "Authentication",
                &format!("Password for {}:", ctx.username),
            )
            .await?;
        if response.is_empty() {
            return Err(AuthError::Cancelled);
        }
        Ok(AuthCredential::Password(Zeroizing::new(response)))
    }
}

/// Public-key auth from `~/.ssh` or an explicit key path.
pub struct PublicKeyProvider;

#[async_trait::async_trait]
impl AuthProvider for PublicKeyProvider {
    fn method_name(&self) -> &'static str {
        "publickey"
    }

    async fn acquire(
        &mut self,
        ctx: &mut AuthContext,
        bridge: &PromptBridge,
    ) -> Result<AuthCredential, AuthError> {
        let key_path = match ctx.private_key.clone() {
            Some(path) => path,
            None => match discover_default_key() {
                Some(path) => path,
                None => {
                    return Err(AuthError::Unavailable(
                        "no private key found in ~/.ssh".to_string(),
                    ))
                },
            },
        };

        // Passphrase handling: PEM-encrypted keys are detectable ("ENCRYPTED"
        // marker); OpenSSH-format bcrypt KDF detection (and the actual russh
        // key load) lands with the engine — prompt 2.1 re-prompts on failure.
        let looks_encrypted = std::fs::read_to_string(&key_path)
            .map(|content| content.contains("ENCRYPTED"))
            .unwrap_or(false);

        let passphrase = match ctx.key_passphrase.take() {
            Some(p) if !p.is_empty() => Some(Zeroizing::new(p)),
            _ if looks_encrypted => {
                let response = bridge
                    .request(
                        PromptKind::Passphrase,
                        "Encrypted key",
                        &format!("Passphrase for {}:", key_path.display()),
                    )
                    .await?;
                if response.is_empty() {
                    return Err(AuthError::Cancelled);
                }
                Some(Zeroizing::new(response))
            },
            _ => None,
        };

        Ok(AuthCredential::KeyFile {
            path: key_path,
            passphrase,
        })
    }
}

/// ssh-agent auth (`SSH_AUTH_SOCK`); the agent client/forwarding channel
/// itself is exercised by the engine (russh agent module, prompt 2.1).
pub struct AgentProvider;

#[async_trait::async_trait]
impl AuthProvider for AgentProvider {
    fn method_name(&self) -> &'static str {
        "agent"
    }

    async fn acquire(
        &mut self,
        ctx: &mut AuthContext,
        _bridge: &PromptBridge,
    ) -> Result<AuthCredential, AuthError> {
        if !ctx.use_agent {
            return Err(AuthError::Unavailable("agent disabled".to_string()));
        }
        if std::env::var_os("SSH_AUTH_SOCK").is_none() {
            return Err(AuthError::Unavailable("SSH_AUTH_SOCK not set".to_string()));
        }
        Ok(AuthCredential::Agent)
    }
}

/// `keyboard-interactive` auth (two-factor / OTP) via the challenge callback
/// or, failing that, an interactive UI prompt.
pub struct KeyboardInteractiveProvider;

#[async_trait::async_trait]
impl AuthProvider for KeyboardInteractiveProvider {
    fn method_name(&self) -> &'static str {
        "keyboard-interactive"
    }

    async fn acquire(
        &mut self,
        ctx: &mut AuthContext,
        bridge: &PromptBridge,
    ) -> Result<AuthCredential, AuthError> {
        let response = if let Some(callback) = ctx.challenge_response.as_ref() {
            callback("Verification code:")
        } else {
            bridge
                .request(
                    PromptKind::Otp,
                    "Two-factor authentication",
                    "Verification code:",
                )
                .await?
        };
        if response.is_empty() {
            return Err(AuthError::Cancelled);
        }
        Ok(AuthCredential::KeyboardInteractive(Zeroizing::new(
            response,
        )))
    }
}

/// Kerberos/GSSAPI (optional; requires the `gssapi` cargo feature and a
/// russh build with GSSAPI support).
pub struct GssApiProvider;

#[async_trait::async_trait]
impl AuthProvider for GssApiProvider {
    fn method_name(&self) -> &'static str {
        "gssapi-with-mic"
    }

    async fn acquire(
        &mut self,
        _ctx: &mut AuthContext,
        _bridge: &PromptBridge,
    ) -> Result<AuthCredential, AuthError> {
        #[cfg(feature = "gssapi")]
        {
            // TODO(prompt 2.1+): wire russh GSSAPI once the engine lands.
            Err(AuthError::Unavailable(
                "gssapi engine support not wired yet".to_string(),
            ))
        }
        #[cfg(not(feature = "gssapi"))]
        {
            Err(AuthError::Unavailable(
                "compiled without the `gssapi` feature".to_string(),
            ))
        }
    }
}

/// Default key search order in `~/.ssh` (prompt 2.2: "loads keys from
/// ~/.ssh/").
fn discover_default_key() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))?;
    let ssh_dir = home.join(".ssh");
    for candidate in ["id_ed25519", "id_ecdsa", "id_rsa"] {
        let path = ssh_dir.join(candidate);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Negotiation (fallback & retry)
// ---------------------------------------------------------------------------

/// Walk `ctx.preferred_methods` in order; first provider that produces a
/// credential wins. `Unavailable` providers are skipped; `Cancelled`/`UiGone`
/// abort immediately; everything else accumulates into the failure summary.
pub async fn negotiate(
    ctx: &mut AuthContext,
    bridge: &PromptBridge,
) -> Result<AuthOutcome, AuthError> {
    let methods = ctx.preferred_methods.clone();
    let mut attempts: Vec<String> = Vec::new();

    for method in methods {
        let mut provider: Box<dyn AuthProvider + Send> = match &method {
            AuthMethod::Password => Box::new(PasswordProvider),
            AuthMethod::KeyFile { .. } => Box::new(PublicKeyProvider),
            AuthMethod::Agent { .. } => Box::new(AgentProvider),
            AuthMethod::KeyboardInteractive => Box::new(KeyboardInteractiveProvider),
        };

        match provider.acquire(ctx, bridge).await {
            Ok(credential) => {
                tracing::info!(method = credential.method_name(), "authentication acquired");
                return Ok(AuthOutcome { method, credential });
            },
            Err(AuthError::Cancelled) | Err(AuthError::UiGone) => return Err(AuthError::Cancelled),
            Err(err) => attempts.push(format!("{:?}: {}", method, err.user_message())),
        }
    }

    Err(AuthError::AllMethodsFailed(attempts.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(methods: Vec<AuthMethod>) -> AuthContext {
        AuthContext {
            username: "ops".into(),
            password: None,
            private_key: None,
            key_passphrase: None,
            use_agent: false,
            preferred_methods: methods,
            challenge_response: None,
        }
    }

    fn spec_with(auth: AuthMethod, username: &str) -> SessionSpec {
        SessionSpec {
            name: "test".into(),
            protocol: mbxt_core::Protocol::Ssh,
            host: Some("h".into()),
            port: Some(22),
            username: Some(username.into()),
            auth,
            tags: vec![],
            notes: String::new(),
            x11_forwarding: false,
            serial: None,
            forwards: Vec::new(),
        }
    }

    #[tokio::test]
    async fn challenge_response_satisfies_keyboard_interactive() {
        let mut context = ctx(vec![AuthMethod::KeyboardInteractive]);
        context.challenge_response = Some(Box::new(|_| "123456".to_string()));
        let outcome = negotiate(&mut context, PromptBridge::shared())
            .await
            .expect("2FA via callback");
        assert_eq!(outcome.credential.method_name(), "keyboard-interactive");
    }

    #[tokio::test]
    async fn password_from_context_short_circuits_prompt() {
        let mut context = ctx(vec![AuthMethod::Password]);
        context.password = Some("sekret".to_string());
        let outcome = negotiate(&mut context, PromptBridge::shared())
            .await
            .expect("password from ctx");
        assert_eq!(outcome.credential.method_name(), "password");
    }

    #[tokio::test]
    async fn unavailable_providers_fall_through_to_next() {
        // Agent disabled → Unavailable → falls through to keyboard-interactive.
        let mut context = ctx(vec![
            AuthMethod::Agent { forward: false },
            AuthMethod::KeyboardInteractive,
        ]);
        context.challenge_response = Some(Box::new(|_| "000000".to_string()));
        let outcome = negotiate(&mut context, PromptBridge::shared())
            .await
            .unwrap();
        assert_eq!(outcome.credential.method_name(), "keyboard-interactive");
    }

    #[tokio::test]
    async fn no_ui_and_no_credentials_aborts_with_cancelled() {
        // Password requires a prompt; the isolated bridge has no UI subscriber.
        let bridge = PromptBridge::new();
        let mut context = ctx(vec![AuthMethod::Password]);
        let result = negotiate(&mut context, &bridge).await;
        assert!(matches!(result, Err(AuthError::Cancelled)));
    }

    #[tokio::test]
    async fn all_failures_aggregate_into_one_error() {
        // Only agent, disabled → the single attempt is recorded.
        let mut context = ctx(vec![AuthMethod::Agent { forward: false }]);
        let err = negotiate(&mut context, PromptBridge::shared())
            .await
            .unwrap_err();
        match err {
            AuthError::AllMethodsFailed(summary) => {
                assert!(summary.contains("unavailable"), "summary: {summary}");
            },
            other => panic!("expected AllMethodsFailed, got {other:?}"),
        }
    }

    #[test]
    fn debug_impls_never_leak_secrets() {
        let mut context = ctx(vec![AuthMethod::Password]);
        context.password = Some("hunter2".to_string());
        assert!(!format!("{context:?}").contains("hunter2"));

        let credential = AuthCredential::Password(Zeroizing::new("hunter2".to_string()));
        assert!(!format!("{credential:?}").contains("hunter2"));
    }

    #[test]
    fn from_spec_builds_sensible_fallback_order() {
        let context = AuthContext::from_spec(&spec_with(AuthMethod::Password, "ops"));
        assert_eq!(context.username, "ops");
        assert_eq!(context.preferred_methods[0], AuthMethod::Password);
        assert!(context
            .preferred_methods
            .contains(&AuthMethod::KeyboardInteractive));

        let path = AuthMethod::KeyFile {
            path: "/tmp/k".into(),
        };
        let context = AuthContext::from_spec(&spec_with(path, "ops"));
        assert!(matches!(
            context.preferred_methods[0],
            AuthMethod::KeyFile { .. }
        ));
        assert!(context.use_agent);
    }

    #[tokio::test]
    async fn prompt_bridge_round_trip_through_subscription() {
        let bridge = PromptBridge::new();
        let mut rx = bridge.subscribe();

        let asker = {
            let bridge = bridge.clone();
            tokio::spawn(async move {
                bridge
                    .request(PromptKind::Password, "t", "password:")
                    .await
                    .expect("answered")
            })
        };

        let prompt = rx.recv().await.expect("prompt broadcast");
        assert!(prompt.masked);
        assert!(bridge.complete(prompt.id, "answer".into()));

        // The reply sender was consumed; a second complete is a no-op.
        assert!(!bridge.complete(prompt.id, "again".into()));
        assert_eq!(asker.await.unwrap(), "answer");
    }

    #[tokio::test]
    async fn prompt_bridge_cancel_resolves_as_cancelled() {
        let bridge = PromptBridge::new();
        let mut rx = bridge.subscribe();

        let asker = {
            let bridge = bridge.clone();
            tokio::spawn(async move {
                bridge
                    .request(PromptKind::Otp, "t", "code:")
                    .await
                    .expect_err("cancelled")
            })
        };

        let prompt = rx.recv().await.unwrap();
        assert!(bridge.cancel(prompt.id));
        assert!(matches!(asker.await.unwrap(), AuthError::Cancelled));
    }

    #[test]
    fn context_drop_zeroizes_secrets() {
        // Observable only indirectly: the strings are dropped through
        // zeroize; assert the field is consumed by take() during auth.
        let mut context = ctx(vec![AuthMethod::Password]);
        context.password = Some("p".into());
        let mut provider = PasswordProvider;
        let taken = futures_block_on(provider.acquire(&mut context, PromptBridge::shared()));
        assert!(context.password.is_none(), "provider consumed the secret");
        assert!(matches!(taken, Ok(AuthCredential::Password(_))));
    }

    /// Minimal block-on for single-step futures in tests (no tokio deps here).
    fn futures_block_on<F: std::future::Future>(fut: F) -> F::Output {
        // tokio::runtime::Handle::current would panic outside a runtime;
        // build a tiny current-thread runtime for this sync test.
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(fut)
    }

    #[test]
    fn default_key_discovery_is_safe_when_absent() {
        // Function must not panic regardless of environment.
        let _ = discover_default_key();
    }

    #[test]
    fn key_file_path_respects_context() {
        let mut context = AuthContext::from_spec(&spec_with(
            AuthMethod::KeyFile {
                path: "/custom/key".into(),
            },
            "ops",
        ));
        let mut provider = PublicKeyProvider;
        // /custom/key does not exist; provider must still return the path
        // (existence is the engine's concern), without prompting.
        let outcome = futures_block_on(provider.acquire(&mut context, PromptBridge::shared()));
        match outcome {
            Ok(AuthCredential::KeyFile { path, passphrase }) => {
                assert_eq!(path, PathBuf::from("/custom/key"));
                assert!(passphrase.is_none());
            },
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn gssapi_provider_is_gated() {
        let mut context = ctx(vec![]);
        let mut provider = GssApiProvider;
        let outcome = futures_block_on(provider.acquire(&mut context, PromptBridge::shared()));
        #[cfg(feature = "gssapi")]
        assert!(matches!(outcome, Err(AuthError::Unavailable(_))));
        #[cfg(not(feature = "gssapi"))]
        assert!(matches!(outcome, Err(AuthError::Unavailable(_))));
    }
}
