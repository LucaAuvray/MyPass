/// Password strength evaluation using zxcvbn-like heuristics.
/// Full zxcvbn implementation would be in Rust, but for MVP
/// we use the entropy-based approach in commands/generator.rs.
///
/// This module provides HIBP integration functionality.

/// Check a SHA-1 password hash prefix against Have I Been Pwned API
/// using k-anonymity (only the first 5 hex chars are sent to the API).
pub async fn check_hibp_prefix(prefix: &str) -> Result<Vec<String>, String> {
    let url = format!("https://api.pwnedpasswords.com/range/{}", prefix);
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Add-Padding", "true")
        .send()
        .await
        .map_err(|e| format!("HIBP request failed: {e}"))?;

    let body = response
        .text()
        .await
        .map_err(|e| format!("HIBP read failed: {e}"))?;

    // Each line: HASH_SUFFIX:COUNT
    let suffixes: Vec<String> = body
        .lines()
        .map(|line| line.to_string())
        .collect();

    Ok(suffixes)
}
