//! Validation of the WebAuthn options the extension forwards. The page wrote
//! them (the extension intercepts `navigator.credentials` before the browser
//! checks anything), so nothing here is trusted except the origin, which the
//! extension's content script supplies.

use super::webauthn::{from_b64url, ES256};
use serde_json::Value;
use std::time::Duration;

// Error codes the extension maps to DOMExceptions (content/passkeys.js).
pub const ATTESTATION_NOT_SUPPORTED: u8 = 20;
pub const CREDENTIAL_IS_EXCLUDED: u8 = 21;
pub const EMPTY_PUBLIC_KEY: u8 = 24;
pub const INVALID_URL_PROVIDED: u8 = 25;
pub const DOMAIN_IS_NOT_VALID: u8 = 27;
pub const DOMAIN_RPID_MISMATCH: u8 = 28;
pub const NO_SUPPORTED_ALGORITHMS: u8 = 29;
pub const UNKNOWN_ERROR: u8 = 31;
pub const INVALID_CHALLENGE: u8 = 32;
pub const INVALID_USER_ID: u8 = 33;

pub struct RegisterRequest {
    pub rp_id: String,
    pub rp_name: String,
    /// base64url, as received; stored as the passkey's user handle.
    pub user_id: String,
    pub user_name: String,
    /// base64url, as received.
    pub challenge: String,
    pub exclude: Vec<Vec<u8>>,
    pub timeout: Duration,
}

pub struct GetRequest {
    pub rp_id: String,
    pub challenge: String,
    pub allow: Vec<Vec<u8>>,
    pub timeout: Duration,
}

/// The relying party id a page at `origin` may use: `requested` (or the
/// origin's host) must be that host or a parent domain of it. Like the
/// browser, refuses non-https origins, IP addresses and single labels.
// shortcut: "contains a dot" stands in for KeePassXC's Public Suffix List check (accepts co.uk, github.io); add a PSL crate if a site ever needs it.
pub fn rp_id_for(origin: &str, requested: Option<&str>) -> Result<String, u8> {
    let url = url::Url::parse(origin).map_err(|_| INVALID_URL_PROVIDED)?;
    if url.scheme() != "https" {
        return Err(INVALID_URL_PROVIDED);
    }
    let host = match url.host() {
        Some(url::Host::Domain(d)) => d.to_string(),
        Some(_) => return Err(DOMAIN_IS_NOT_VALID),
        None => return Err(INVALID_URL_PROVIDED),
    };
    let rp_id = requested.filter(|r| !r.is_empty()).map(str::to_lowercase).unwrap_or_else(|| host.clone());
    if !rp_id.contains('.') || rp_id.parse::<std::net::IpAddr>().is_ok() {
        return Err(DOMAIN_IS_NOT_VALID);
    }
    if host == rp_id || host.ends_with(&format!(".{rp_id}")) {
        Ok(rp_id)
    } else {
        Err(DOMAIN_RPID_MISMATCH)
    }
}

pub fn parse_register(options: &Value, origin: &str) -> Result<RegisterRequest, u8> {
    require_options(options)?;
    let rp = &options["rp"];
    let rp_id = rp_id_for(origin, rp.get("id").and_then(Value::as_str))?;
    let challenge = challenge(options)?;

    let user = &options["user"];
    let user_id = user.get("id").and_then(Value::as_str).unwrap_or("");
    if !from_b64url(user_id).is_some_and(|id| (1..=64).contains(&id.len())) {
        return Err(INVALID_USER_ID);
    }

    let es256 = options["pubKeyCredParams"]
        .as_array()
        .is_some_and(|params| params.iter().any(|p| p.get("alg").and_then(Value::as_i64) == Some(ES256)));
    if !es256 {
        return Err(NO_SUPPORTED_ALGORITHMS);
    }
    // direct/indirect get "none" (allowed by the spec); enterprise would be a lie.
    if options.get("attestation").and_then(Value::as_str) == Some("enterprise") {
        return Err(ATTESTATION_NOT_SUPPORTED);
    }

    let rp_name = rp.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()).unwrap_or(&rp_id).to_string();
    Ok(RegisterRequest {
        rp_name,
        rp_id,
        user_id: user_id.to_string(),
        user_name: user.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
        challenge,
        exclude: credential_ids(options, "excludeCredentials"),
        timeout: timeout(options),
    })
}

pub fn parse_get(options: &Value, origin: &str) -> Result<GetRequest, u8> {
    require_options(options)?;
    let rp_id = rp_id_for(origin, options.get("rpId").and_then(Value::as_str))?;
    Ok(GetRequest {
        rp_id,
        challenge: challenge(options)?,
        allow: credential_ids(options, "allowCredentials"),
        timeout: timeout(options),
    })
}

fn require_options(options: &Value) -> Result<(), u8> {
    match options.as_object() {
        Some(o) if !o.is_empty() => Ok(()),
        _ => Err(EMPTY_PUBLIC_KEY),
    }
}

fn challenge(options: &Value) -> Result<String, u8> {
    let challenge = options.get("challenge").and_then(Value::as_str).unwrap_or("");
    match from_b64url(challenge) {
        Some(bytes) if !bytes.is_empty() => Ok(challenge.to_string()),
        _ => Err(INVALID_CHALLENGE),
    }
}

/// An unreadable id stays in the list as an empty id, which matches no stored
/// passkey: a non-empty allow list never turns into "any account of the site".
fn credential_ids(options: &Value, key: &str) -> Vec<Vec<u8>> {
    options
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|c| c.get("id").and_then(Value::as_str).and_then(from_b64url).unwrap_or_default())
        .collect()
}

/// The extension already bounds it to 15–120 s; enforce it again here.
fn timeout(options: &Value) -> Duration {
    let ms = options.get("timeout").and_then(Value::as_f64).unwrap_or(120_000.0);
    Duration::from_millis((ms as u64).clamp(15_000, 120_000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::time::Duration;

    #[test]
    fn rp_id_rules() {
        assert_eq!(rp_id_for("https://accounts.binance.com", Some("binance.com")), Ok("binance.com".into()));
        assert_eq!(rp_id_for("https://www.paypal.com", None), Ok("www.paypal.com".into()));
        assert_eq!(rp_id_for("https://Accounts.Binance.com", Some("BINANCE.com")), Ok("binance.com".into()));
        assert_eq!(rp_id_for("https://evil.com", Some("binance.com")), Err(28));
        assert_eq!(rp_id_for("https://binance.com.evil.com", Some("binance.com")), Err(28));
        assert_eq!(rp_id_for("https://notbinance.com", Some("binance.com")), Err(28));
        assert_eq!(rp_id_for("https://binance.com", Some("binance.com.")), Err(28));
        assert_eq!(rp_id_for("https://x.com", Some("com")), Err(27));
        assert_eq!(rp_id_for("https://192.168.1.10", None), Err(27));
        assert_eq!(rp_id_for("http://binance.com", None), Err(25));
        assert_eq!(rp_id_for("not a url", None), Err(25));
    }

    fn reg_opts() -> Value {
        json!({
            "rp": {"id": "binance.com", "name": "Binance"},
            "user": {"id": "dXNlcjE", "name": "luca@example.com", "displayName": "Luca"},
            "challenge": "Y2hhbGxlbmdl",
            "pubKeyCredParams": [{"type": "public-key", "alg": -257}, {"type": "public-key", "alg": -7}],
            "excludeCredentials": [{"id": "AQID", "type": "public-key"}],
            "attestation": "direct",
            "timeout": 60000
        })
    }

    #[test]
    fn parse_register_ok() {
        let r = parse_register(&reg_opts(), "https://accounts.binance.com").unwrap();
        assert_eq!((r.rp_id.as_str(), r.rp_name.as_str()), ("binance.com", "Binance"));
        assert_eq!((r.user_id.as_str(), r.user_name.as_str()), ("dXNlcjE", "luca@example.com"));
        assert_eq!(r.challenge, "Y2hhbGxlbmdl");
        assert_eq!(r.exclude, vec![vec![1u8, 2, 3]]);
        assert_eq!(r.timeout, Duration::from_secs(60));
    }

    #[test]
    fn parse_register_errors_and_timeout_bounds() {
        let with = |k: &str, v: Value| {
            let mut o = reg_opts();
            o[k] = v;
            parse_register(&o, "https://accounts.binance.com")
        };
        assert_eq!(with("pubKeyCredParams", json!([{"type": "public-key", "alg": -257}])).err(), Some(29));
        assert_eq!(with("challenge", json!("")).err(), Some(32));
        assert_eq!(with("user", json!({"id": "", "name": "x"})).err(), Some(33));
        assert_eq!(with("user", json!({"id": super::super::webauthn::b64url(&[0u8; 65]), "name": "x"})).err(), Some(33));
        assert_eq!(with("attestation", json!("enterprise")).err(), Some(20));
        assert_eq!(parse_register(&json!({}), "https://a.com").err(), Some(24));
        assert_eq!(with("timeout", json!(500_000)).unwrap().timeout, Duration::from_secs(120));
        assert_eq!(with("timeout", json!(1_000)).unwrap().timeout, Duration::from_secs(15));
        assert_eq!(with("timeout", Value::Null).unwrap().timeout, Duration::from_secs(120));
    }

    #[test]
    fn parse_get_ok_and_errors() {
        let o = json!({"rpId": "binance.com", "challenge": "Y2hhbGxlbmdl",
                       "allowCredentials": [{"id": "AQID", "type": "public-key"}]});
        let g = parse_get(&o, "https://www.binance.com").unwrap();
        assert_eq!((g.rp_id.as_str(), g.challenge.as_str()), ("binance.com", "Y2hhbGxlbmdl"));
        assert_eq!(g.allow, vec![vec![1u8, 2, 3]]);
        assert_eq!(g.timeout, Duration::from_secs(120));
        assert_eq!(
            parse_get(&json!({"rpId": "binance.com", "challenge": ""}), "https://www.binance.com").err(),
            Some(32)
        );
        assert_eq!(
            parse_get(&json!({"rpId": "paypal.com", "challenge": "eA"}), "https://www.binance.com").err(),
            Some(28)
        );
    }
}
