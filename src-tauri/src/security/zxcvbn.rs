/// Security module — placeholder for password health checks and zxcvbn.
/// Full implementation uses the entropy-based evaluator in commands/generator.rs.

pub fn is_password_compromised(_password: &str) -> bool {
    // Stub: full implementation would check local bloom filter or HIBP
    false
}

pub fn is_password_weak(password: &str) -> bool {
    password.len() < 8
}

pub fn is_password_reused(password: &str, existing_passwords: &[String]) -> bool {
    existing_passwords.contains(&password.to_string())
}
