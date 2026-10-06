//! Pure X11 display + Xauthority parsing (Prompt 4.1, no I/O).
//!
//! - [`parse_display`] turns `$DISPLAY`-style values (`:0`, `:0.0`,
//!   `localhost:10.0`, `/tmp/.X11-unix/X0`) into [`DisplayInfo`].
//! - [`local_socket_path`] maps a local display to its Unix socket
//!   (`/tmp/.X11-unix/X<n>` — covers X11 and XWayland, which share the path
//!   scheme).
//! - [`parse_xauthority`] decodes `~/.Xauthority` bytes into [`XAuthEntry`];
//!   [`cookie_for_display`] picks the `MIT-MAGIC-COOKIE-1` entry for a
//!   display number.

/// Parsed display address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayInfo {
    /// Display number (`:10` → 10).
    pub display_number: u32,
    /// Screen number after the dot (`:0.0` → 0).
    pub screen: u32,
    /// Whether the address names this machine (unix socket eligible).
    pub is_local: bool,
    /// Original string (round-trip / diagnostics).
    pub raw: String,
}

/// X11 parse failures (user-actionable strings upstream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayParseError {
    Empty,
    BadFormat(String),
}

impl std::fmt::Display for DisplayParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "DISPLAY is empty"),
            Self::BadFormat(value) => write!(f, "unparsable DISPLAY value: {value}"),
        }
    }
}

/// Parse `$DISPLAY`-style values.
///
/// Accepts `:N[.S]`, `host:N[.S]` (`localhost`, `unix`, empty host, or any
/// hostname), and literal socket paths `/tmp/.X11-unix/XN`.
pub fn parse_display(raw: &str) -> Result<DisplayInfo, DisplayParseError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(DisplayParseError::Empty);
    }
    if let Some(number) = raw
        .strip_prefix("/tmp/.X11-unix/X")
        .or_else(|| raw.strip_prefix("/tmp/.X11-unix/X"))
    {
        let display_number = number
            .parse::<u32>()
            .map_err(|_| DisplayParseError::BadFormat(raw.to_string()))?;
        return Ok(DisplayInfo {
            display_number,
            screen: 0,
            is_local: true,
            raw: raw.to_string(),
        });
    }
    let after_host = raw
        .rsplit(':')
        .next()
        .ok_or_else(|| DisplayParseError::BadFormat(raw.to_string()))?;
    if after_host.is_empty() {
        return Err(DisplayParseError::BadFormat(raw.to_string()));
    }
    let host_part = raw.rsplit_once(':').map(|(host, _)| host).unwrap_or("");
    let (display_part, screen) = match after_host.split_once('.') {
        Some((display, screen)) => {
            let screen = screen
                .parse::<u32>()
                .map_err(|_| DisplayParseError::BadFormat(raw.to_string()))?;
            (display, screen)
        },
        None => (after_host, 0),
    };
    let display_number = display_part
        .parse::<u32>()
        .map_err(|_| DisplayParseError::BadFormat(raw.to_string()))?;
    let is_local = matches!(host_part, "" | "localhost" | "unix" | "127.0.0.1" | "::1");
    Ok(DisplayInfo {
        display_number,
        screen,
        is_local,
        raw: raw.to_string(),
    })
}

/// Unix socket for a local display (`/tmp/.X11-unix/X<n>`).
/// Returns `None` for remote (TCP) displays — those need `host:6000+n`.
pub fn local_socket_path(info: &DisplayInfo) -> Option<std::path::PathBuf> {
    info.is_local
        .then(|| std::path::PathBuf::from(format!("/tmp/.X11-unix/X{}", info.display_number)))
}

/// TCP endpoint `(host, port)` for a remote display (`host:6000+n`).
/// Returns `None` for local displays (use the unix socket instead).
pub fn tcp_endpoint(raw_host: &str, info: &DisplayInfo) -> Option<(String, u16)> {
    if info.is_local {
        return None;
    }
    let host = raw_host
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(raw_host);
    let port = 6000u16.checked_add(info.display_number as u16)?;
    Some((host.to_string(), port))
}

/// One `~/.Xauthority` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XAuthEntry {
    /// Address family (0 = Internet, 256 = local, 65535 = wild).
    pub family: u16,
    pub address: Vec<u8>,
    /// Display number as ASCII (`"10"`).
    pub number: String,
    /// Auth scheme (`MIT-MAGIC-COOKIE-1`).
    pub protocol: String,
    /// Raw cookie bytes.
    pub cookie: Vec<u8>,
}

/// Decode `~/.Xauthority` bytes (big-endian u16-prefixed fields).
///
/// Trailing truncated records are ignored (a concurrently rewritten file
/// must not fail the whole lookup).
pub fn parse_xauthority(bytes: &[u8]) -> Vec<XAuthEntry> {
    let mut entries = Vec::new();
    let mut cursor = bytes;
    while !cursor.is_empty() {
        let Some((family, rest)) = take_u16(cursor) else {
            break;
        };
        let Some((address, rest)) = take_bytes(rest) else {
            break;
        };
        let Some((number, rest)) = take_bytes(rest) else {
            break;
        };
        let Some((protocol, rest)) = take_bytes(rest) else {
            break;
        };
        let Some((cookie, rest)) = take_bytes(rest) else {
            break;
        };
        entries.push(XAuthEntry {
            family,
            address: address.to_vec(),
            number: String::from_utf8_lossy(number).into_owned(),
            protocol: String::from_utf8_lossy(protocol).into_owned(),
            cookie: cookie.to_vec(),
        });
        cursor = rest;
    }
    entries
}

/// Pick the `MIT-MAGIC-COOKIE-1` cookie for `display_number`, if any.
pub fn cookie_for_display(entries: &[XAuthEntry], display_number: u32) -> Option<&[u8]> {
    let wanted = display_number.to_string();
    entries
        .iter()
        .find(|entry| entry.number == wanted && entry.protocol == "MIT-MAGIC-COOKIE-1")
        .map(|entry| entry.cookie.as_slice())
}

/// Lowercase hex encoding (SSH `request_x11` cookie wire format).
pub fn hex_cookie(cookie: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(cookie.len() * 2);
    for byte in cookie {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0F) as usize] as char);
    }
    out
}

fn take_u16(bytes: &[u8]) -> Option<(u16, &[u8])> {
    let (head, rest) = bytes.split_at_checked(2)?;
    Some((u16::from_be_bytes([head[0], head[1]]), rest))
}

fn take_bytes(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let (len, rest) = take_u16(bytes)?;
    rest.split_at_checked(len as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_forms_parse() {
        let info = parse_display(":0").unwrap();
        assert_eq!((info.display_number, info.screen), (0, 0));
        assert!(info.is_local);

        let info = parse_display(":0.1").unwrap();
        assert_eq!((info.display_number, info.screen), (0, 1));

        let info = parse_display("localhost:10.0").unwrap();
        assert_eq!((info.display_number, info.screen), (10, 0));
        assert!(info.is_local);

        let info = parse_display("/tmp/.X11-unix/X2").unwrap();
        assert_eq!(info.display_number, 2);

        let info = parse_display("remote.example:3.0").unwrap();
        assert!(!info.is_local);
        assert_eq!(
            tcp_endpoint("remote.example:3.0", &info),
            Some(("remote.example".to_string(), 6003))
        );

        assert_eq!(parse_display(""), Err(DisplayParseError::Empty));
        assert!(parse_display("nope").is_err());
        assert!(parse_display(":").is_err());
    }

    #[test]
    fn local_socket_paths_cover_x11_and_xwayland() {
        let info = parse_display(":0").unwrap();
        assert_eq!(
            local_socket_path(&info),
            Some(std::path::PathBuf::from("/tmp/.X11-unix/X0"))
        );
        let remote = parse_display("remote:1").unwrap();
        assert_eq!(local_socket_path(&remote), None);
    }

    fn entry_bytes(
        family: u16,
        address: &[u8],
        number: &str,
        protocol: &str,
        cookie: &[u8],
    ) -> Vec<u8> {
        let mut out = Vec::new();
        for chunk in [address, number.as_bytes(), protocol.as_bytes(), cookie] {
            out.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
            out.extend_from_slice(chunk);
        }
        let mut prefixed = (family.to_be_bytes()).to_vec();
        prefixed.extend(out);
        prefixed
    }

    #[test]
    fn xauthority_round_trip_and_lookup() {
        let mut blob = entry_bytes(256, b"host", "10", "MIT-MAGIC-COOKIE-1", b"\x01\x02\xab");
        blob.extend(entry_bytes(
            0,
            b"\x7f\x00\x00\x01",
            "10",
            "XDM-AUTHORIZATION-1",
            b"zz",
        ));
        blob.extend(b"\x00"); // truncated tail is ignored
        let entries = parse_xauthority(&blob);
        assert_eq!(entries.len(), 2);
        assert_eq!(
            cookie_for_display(&entries, 10),
            Some(b"\x01\x02\xab".as_slice())
        );
        assert_eq!(cookie_for_display(&entries, 11), None);
        assert_eq!(hex_cookie(b"\x01\x02\xab"), "0102ab");
    }
}
