//! TOTP (Time-based One-Time Password) operations.
use totp_rs::{Algorithm, TOTP};

#[derive(serde::Deserialize)]
pub struct TotpSetup { pub secret: String, pub algorithm: Option<String>, pub digits: Option<usize>, pub period: Option<u32> }

#[derive(serde::Serialize)]
pub struct TotpCode { pub code: String, pub seconds_remaining: u32 }

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
    Ok(TotpCode { code, seconds_remaining: (period_val - (now % period_val)) as u32 })
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

    #[test]
    fn totp_code_is_six_digits_with_valid_window() {
        let secret = generate_totp_secret().unwrap();
        let out = generate_totp_code(secret, None, None, None).unwrap();
        assert_eq!(out.code.len(), 6);
        assert!(out.code.chars().all(|c| c.is_ascii_digit()));
        assert!((1..=30).contains(&out.seconds_remaining));
    }
}
