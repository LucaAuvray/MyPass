//! TOTP (Time-based One-Time Password) operations.
use crate::xml::Entry;
use totp_rs::{Algorithm, TOTP};

#[derive(serde::Deserialize)]
pub struct TotpSetup { pub secret: String, pub algorithm: Option<String>, pub digits: Option<usize>, pub period: Option<u32> }

#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TotpCode {
    pub code: String,
    pub period: u64,
    pub seconds_remaining: u64,
}

/// The 2FA link of an entry: its `otp` field (KeePassXC), else one rebuilt from
/// the KeePass2 `TOTP Seed` + `TOTP Settings` ("<period>;<digits>") fields.
pub fn entry_otp_uri(entry: &Entry) -> Option<String> {
    let field = |key: &str| entry.get_field(key).map(str::trim).filter(|v| !v.is_empty());
    if let Some(otp) = field("otp") {
        return Some(otp.to_string());
    }
    let mut uri = normalize(field("TOTP Seed")?, entry.title());
    // An unreadable or Steam ("30;S") setting keeps the defaults.
    if let Some((period, digits)) = field("TOTP Settings").and_then(|s| s.split_once(';')) {
        if let (Ok(p), Ok(d)) = (period.trim().parse::<u64>(), digits.trim().parse::<u8>()) {
            uri.push_str(&format!("&period={p}&digits={d}"));
        }
    }
    Some(uri)
}

/// Code of a 2FA link at `now` (unix seconds). Short keys (80 bits) are accepted;
/// anything that would not give a real code is `TOTP_INVALID`, never a panic.
pub fn code_at(uri: &str, now: u64) -> Result<TotpCode, String> {
    let invalid = || "TOTP_INVALID".to_string();
    let totp = TOTP::from_url_unchecked(uri).map_err(|_| invalid())?;
    if totp.secret.is_empty() || !(6..=8).contains(&totp.digits) || totp.step == 0 {
        return Err(invalid());
    }
    Ok(TotpCode { code: totp.generate(now), period: totp.step, seconds_remaining: totp.step - now % totp.step })
}

/// Code of an entry's 2FA at `now`; `TOTP_NONE` when it has none.
pub fn code_for_entry(entry: &Entry, now: u64) -> Result<TotpCode, String> {
    code_at(&entry_otp_uri(entry).ok_or("TOTP_NONE")?, now)
}

/// TOTP as stored in the `otp` field (KeePassXC convention). A bare key is
/// wrapped into an `otpauth://` link; a pasted link gets its `secret` cleaned
/// (spaces, `=` padding, lowercase) and is otherwise kept as is.
pub fn normalize(value: &str, title: &str) -> String {
    let v = value.trim();
    if v.is_empty() {
        return String::new();
    }
    if v.get(..10).is_some_and(|p| p.eq_ignore_ascii_case("otpauth://")) {
        return clean_link_secret(v).unwrap_or_else(|| v.to_string());
    }
    format!("otpauth://totp/{}?secret={}", percent_encode(title), clean_secret(v))
}

fn clean_secret(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && *c != '=').collect::<String>().to_uppercase()
}

/// The link with a cleaned `secret`, or `None` when it does not parse or is already clean.
fn clean_link_secret(link: &str) -> Option<String> {
    let mut url = url::Url::parse(link).ok()?;
    let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    let (_, secret) = pairs.iter().find(|(k, _)| k == "secret")?;
    if clean_secret(secret) == *secret {
        return None;
    }
    url.query_pairs_mut().clear().extend_pairs(
        pairs.iter().map(|(k, v)| (k.as_str(), if k == "secret" { clean_secret(v) } else { v.clone() })),
    );
    Some(url.to_string())
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

pub fn generate_totp_code(secret: String, algorithm: Option<String>, digits: Option<usize>, period: Option<u32>) -> Result<TotpCode, String> {
    let algo = match algorithm.as_deref() { Some("SHA256") => Algorithm::SHA256, Some("SHA512") => Algorithm::SHA512, _ => Algorithm::SHA1 };
    let digits_val = digits.unwrap_or(6);
    let period_val = period.unwrap_or(30) as u64;
    let secret_bytes = base32_decode(&secret).map_err(|e| format!("Invalid TOTP secret: {e}"))?;
    let totp = TOTP::new(algo, digits_val, 1, period_val, secret_bytes, None, String::new()).map_err(|e| format!("Failed to create TOTP: {e}"))?;
    // totp.generate_current() appelle SystemTime::now() en interne (panic wasm)
    // → on fournit notre heure via generate(now).
    let now = crate::time::unix_now();
    let code = totp.generate(now);
    Ok(TotpCode { code, period: period_val, seconds_remaining: period_val - (now % period_val) })
}

pub fn generate_totp_secret() -> Result<String, String> {
    let mut s = vec![0u8; 20]; rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut s);
    Ok(base32_encode(&s))
}

fn base32_encode(d: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut r = String::new(); let mut b = 0u32; let mut n = 0u32;
    for &x in d { b = (b << 8) | x as u32; n += 8; while n >= 5 { n -= 5; r.push(A[((b >> n) & 0x1F) as usize] as char); } }
    if n > 0 { r.push(A[((b << (5 - n)) & 0x1F) as usize] as char); }
    r
}

fn base32_decode(e: &str) -> Result<Vec<u8>, String> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let e = e.trim_end_matches('=').to_uppercase();
    let mut r = Vec::new(); let mut b = 0u32; let mut n = 0u32;
    for c in e.chars() {
        let i = A.iter().position(|&x| x == c as u8).ok_or_else(|| format!("Invalid base32 char: {c}"))?;
        b = (b << 5) | i as u32; n += 5;
        if n >= 8 { n -= 8; r.push((b >> n) as u8); }
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::entries::set_string_field;
    use crate::xml::Entry;

    const RFC_SHA1: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    const RFC_SHA256: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA";
    const RFC_SHA512: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNA";

    fn code(uri: &str, now: u64) -> String {
        code_at(uri, now).unwrap().code
    }

    fn entry(fields: &[(&str, &str)]) -> Entry {
        let mut e = Entry::new("Site", "", "", "");
        for (k, v) in fields {
            set_string_field(&mut e, k, v);
        }
        e
    }

    #[test]
    fn rfc6238_vectors() {
        for (now, sha1, sha256, sha512) in [
            (59, "94287082", "46119246", "90693936"),
            (1111111109, "07081804", "68084774", "25091201"),
            (2000000000, "69279037", "90698825", "38618901"),
        ] {
            assert_eq!(code(&format!("otpauth://totp/r?secret={RFC_SHA1}&digits=8"), now), sha1);
            assert_eq!(code(&format!("otpauth://totp/r?secret={RFC_SHA256}&digits=8&algorithm=SHA256"), now), sha256);
            assert_eq!(code(&format!("otpauth://totp/r?secret={RFC_SHA512}&digits=8&algorithm=SHA512"), now), sha512);
        }
    }

    #[test]
    fn short_80_bit_key_and_link_parameters() {
        assert_eq!(code("otpauth://totp/A?secret=JBSWY3DPEHPK3PXP", 1111111109), "071271");
        let c = code_at("otpauth://totp/A?secret=JBSWY3DPEHPK3PXP&digits=8&period=60&algorithm=SHA256", 1111111109).unwrap();
        assert_eq!((c.code.as_str(), c.period, c.seconds_remaining), ("83444909", 60, 60 - 1111111109 % 60));
    }

    #[test]
    fn unusable_links_are_invalid_without_panicking() {
        for uri in [
            "otpauth://totp/A?secret=JBSWY3DPEHPK3PXP&digits=12",
            "otpauth://totp/A?secret=JBSWY3DPEHPK3PXP&period=0",
            "otpauth://totp/A?secret=",
            "otpauth://totp/A",
            "otpauth://hotp/A?secret=JBSWY3DPEHPK3PXP",
            "otpauth://totp/A?secret=JBSW!",
            "pas un lien",
        ] {
            assert_eq!(code_at(uri, 59).err().as_deref(), Some("TOTP_INVALID"), "{uri}");
        }
    }

    #[test]
    fn normalize_cleans_bare_keys_and_pasted_links() {
        assert_eq!(normalize("jbsw y3dp ehpk 3pxp", "Mon site"), "otpauth://totp/Mon%20site?secret=JBSWY3DPEHPK3PXP");
        assert_eq!(normalize("  ", "t"), "");
        // An already clean link comes back as is (only trimmed).
        assert_eq!(normalize(" OTPAUTH://totp/X?secret=AB\n", "t"), "OTPAUTH://totp/X?secret=AB");
        let pasted = normalize("  otpauth://totp/A?secret=jbsw%20y3dp%20ehpk%203pxp%3D%3D&digits=8 \n", "t");
        assert!(pasted.contains("secret=JBSWY3DPEHPK3PXP") && pasted.contains("digits=8"), "{pasted}");
        assert_eq!(code(&pasted, 1111111109), "33071271"); // SHA1, 8 digits, period 30
    }

    #[test]
    fn entry_link_comes_from_otp_or_legacy_fields() {
        let legacy = entry(&[("TOTP Seed", "jbsw y3dp ehpk 3pxp"), ("TOTP Settings", "60;8")]);
        assert_eq!(entry_otp_uri(&legacy).as_deref(), Some("otpauth://totp/Site?secret=JBSWY3DPEHPK3PXP&period=60&digits=8"));
        let steam = entry(&[("TOTP Seed", "JBSWY3DPEHPK3PXP"), ("TOTP Settings", "30;S")]);
        assert_eq!(entry_otp_uri(&steam).as_deref(), Some("otpauth://totp/Site?secret=JBSWY3DPEHPK3PXP"));
        let both = entry(&[("otp", "otpauth://totp/X?secret=AB"), ("TOTP Seed", "JBSWY3DPEHPK3PXP")]);
        assert_eq!(entry_otp_uri(&both).as_deref(), Some("otpauth://totp/X?secret=AB"));
        assert_eq!(entry_otp_uri(&entry(&[("otp", "")])), None);
        assert_eq!(code_for_entry(&entry(&[]), 59).err().as_deref(), Some("TOTP_NONE"));
        assert_eq!(code_for_entry(&legacy, 1111111109).unwrap().code, "97912772"); // SHA1, 8 digits, period 60
    }

    #[test]
    fn totp_code_is_six_digits_with_valid_window() {
        let secret = generate_totp_secret().unwrap();
        let out = generate_totp_code(secret, None, None, None).unwrap();
        assert_eq!(out.code.len(), 6);
        assert!(out.code.chars().all(|c| c.is_ascii_digit()));
        assert!((1..=30).contains(&out.seconds_remaining));
    }
}
