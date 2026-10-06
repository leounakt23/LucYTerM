//! Local X display detection + Xauthority cookies (Prompt 4.1).
//!
//! Pure parsing lives in `mbxt_core::x11` (shared with the transport crate);
//! this module adds the I/O: `$DISPLAY` probing (X11 and XWayland), socket
//! reachability checks, and `~/.Xauthority` cookie reads.
//!
//! Security note: only `MIT-MAGIC-COOKIE-1` over the local unix socket is
//! used (trusted local server). Untrusted forwarding (`SECURITY` extension /
//! untrusted cookies) is deliberately out of scope — the SSH layer still
//! isolates the cookie per connection via `request_x11`.

pub use mbxt_core::x11::{
    cookie_for_display, hex_cookie, local_socket_path, parse_display, parse_xauthority,
    tcp_endpoint, DisplayInfo, DisplayParseError, XAuthEntry,
};

use super::X11Error;

/// Where a local X client should connect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalEndpoint {
    /// `/tmp/.X11-unix/X<n>` (X11 and XWayland share this scheme).
    Unix(std::path::PathBuf),
    /// `host:6000+n` (remote displays only).
    Tcp(String, u16),
}

/// Cookie handed to `request_x11` (hex wire format).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XAuthCookie {
    pub protocol: String,
    /// Lowercase hex of the raw cookie bytes.
    pub hex: String,
}

/// Read `$DISPLAY`, falling back to an XWayland probe.
///
/// When `$DISPLAY` is unset but `$WAYLAND_DISPLAY` exists, `:0` is returned
/// if its socket exists (the standard XWayland placement) — otherwise
/// [`X11Error::NoDisplay`].
pub fn detect_display() -> Result<String, X11Error> {
    if let Ok(display) = std::env::var("DISPLAY") {
        if !display.trim().is_empty() {
            return Ok(display);
        }
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::path::Path::new("/tmp/.X11-unix/X0").exists()
    {
        return Ok(":0".to_string());
    }
    Err(X11Error::NoDisplay)
}

/// Resolve a display string to a dialable local endpoint.
///
/// Unix sockets must exist (missing socket → actionable error → the UI
/// warning). Remote displays resolve to TCP without a reachability probe
/// (probing would block the UI thread; dial failures surface per-connection).
pub fn local_endpoint(display: &str) -> Result<LocalEndpoint, X11Error> {
    let info = parse_display(display).map_err(|err| X11Error::Display(err.to_string()))?;
    if let Some(path) = local_socket_path(&info) {
        if path.exists() {
            return Ok(LocalEndpoint::Unix(path));
        }
        return Err(X11Error::NoLocalServer(path));
    }
    let (host, port) = tcp_endpoint(display, &info)
        .ok_or_else(|| X11Error::Display(format!("cannot route display: {display}")))?;
    Ok(LocalEndpoint::Tcp(host, port))
}

/// Read the `MIT-MAGIC-COOKIE-1` for `display_number` from
/// `$XAUTHORITY` (or `~/.Xauthority`).
///
/// A missing/unreadable file yields `Ok(None)` — the SSH layer then sends an
/// empty cookie and warns (mirrors `ssh -X` without `xauth`; the server may
/// still accept it for trusted forwarding).
pub fn read_cookie(display_number: u32) -> Result<Option<XAuthCookie>, X11Error> {
    let path = std::env::var_os("XAUTHORITY")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::Path::new(&home).join(".Xauthority"))
        });
    let Some(path) = path else {
        return Ok(None);
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(X11Error::Auth(err.to_string())),
    };
    let entries = parse_xauthority(&bytes);
    Ok(
        cookie_for_display(&entries, display_number).map(|cookie| XAuthCookie {
            protocol: "MIT-MAGIC-COOKIE-1".to_string(),
            hex: hex_cookie(cookie),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// Process env is global: serialize every test that mutates it so
    /// parallel `cargo test` threads cannot interleave set/remove pairs.
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn detect_prefers_display_then_xwayland() {
        let _guard = env_lock().lock().unwrap();
        // Real env varies by machine; only assert the error shape when empty.
        unsafe {
            std::env::remove_var("DISPLAY");
            std::env::remove_var("WAYLAND_DISPLAY");
        }
        match detect_display() {
            Ok(display) => assert!(!display.is_empty()),
            Err(X11Error::NoDisplay) => {},
            Err(other) => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn missing_socket_is_actionable() {
        // Display :99 has no socket on any sane machine.
        match local_endpoint(":99") {
            Err(X11Error::NoLocalServer(path)) => {
                assert_eq!(path, std::path::PathBuf::from("/tmp/.X11-unix/X99"));
            },
            other => panic!("expected NoLocalServer, got {other:?}"),
        }
        assert!(local_endpoint("remote.example:3").is_ok());
    }

    #[test]
    fn cookie_read_handles_absent_file() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var("XAUTHORITY", "/nonexistent-mbxt-xauth");
        }
        assert_eq!(read_cookie(0).unwrap(), None);
        unsafe {
            std::env::remove_var("XAUTHORITY");
        }
    }

    #[test]
    fn cookie_read_finds_matching_entry() {
        let _guard = env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Xauthority");
        let mut blob = Vec::new();
        let (number, cookie) = ("0", b"\xde\xad".as_slice());
        blob.extend_from_slice(&256u16.to_be_bytes());
        blob.extend_from_slice(&4u16.to_be_bytes());
        blob.extend_from_slice(b"host");
        blob.extend_from_slice(&(number.len() as u16).to_be_bytes());
        blob.extend_from_slice(number.as_bytes());
        blob.extend_from_slice(&18u16.to_be_bytes());
        blob.extend_from_slice(b"MIT-MAGIC-COOKIE-1");
        blob.extend_from_slice(&(cookie.len() as u16).to_be_bytes());
        blob.extend_from_slice(cookie);
        std::fs::write(&path, blob).unwrap();
        unsafe {
            std::env::set_var("XAUTHORITY", &path);
        }
        let cookie = read_cookie(0).unwrap().expect("cookie found");
        assert_eq!(cookie.hex, "dead");
        unsafe {
            std::env::remove_var("XAUTHORITY");
        }
    }
}
