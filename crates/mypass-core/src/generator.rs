//! Password and passphrase generation, and strength evaluation.
use rand::Rng;

#[derive(serde::Deserialize)]
pub struct PasswordConfig {
    pub length: usize,
    pub uppercase: bool,
    pub lowercase: bool,
    pub digits: bool,
    pub symbols: bool,
    #[serde(default)]
    pub exclude_similar: bool,
    #[serde(default)]
    pub exclude_ambiguous: bool,
}

#[derive(serde::Deserialize)]
pub struct PassphraseConfig {
    pub word_count: usize,
    pub separator: String,
    pub word_case: String, // "lower", "upper", "title"
    #[serde(default)]
    pub include_number: bool,
}

#[derive(serde::Serialize)]
pub struct StrengthResult {
    pub score: u32,
    pub label: String,
    pub color: String,
    pub feedback: String,
    pub crack_time_seconds: f64,
    pub crack_time_display: String,
}

const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()_+-=[]{}|;:,.<>?";
const SIMILAR: &str = "il1Lo0O";
const AMBIGUOUS: &str = "{}[]()/\\'\"`~,;:.<>";

/// Default EFF large wordlist — a small subset for passphrase generation.
const DEFAULT_WORDLIST: &[&str] = &[
    "apple", "banana", "cherry", "dragon", "eagle", "falcon", "guitar", "hammer",
    "island", "jungle", "knight", "lemon", "mango", "nectar", "orange", "puzzle",
    "quartz", "rocket", "sunset", "temple", "umbrella", "violet", "walnut", "xenon",
    "yacht", "zebra", "anchor", "bridge", "castle", "diamond", "emerald", "forest",
    "garden", "harbor", "icicle", "jasmine", "kayak", "lantern", "meadow", "nebula",
    "oasis", "pebble", "quiver", "ribbon", "saddle", "thunder", "unique", "velvet",
    "wander", "zenith", "aurora", "blossom", "compass", "desert", "ember", "frost",
];

pub fn generate_password(config: PasswordConfig) -> Result<String, String> {
    let mut charset = String::new();

    if config.lowercase {
        charset.push_str(LOWERCASE);
    }
    if config.uppercase {
        charset.push_str(UPPERCASE);
    }
    if config.digits {
        charset.push_str(DIGITS);
    }
    if config.symbols {
        charset.push_str(SYMBOLS);
    }

    if charset.is_empty() {
        return Err("At least one character set must be selected".to_string());
    }

    // Filter out similar/ambiguous characters if requested
    if config.exclude_similar {
        charset = charset.chars().filter(|c| !SIMILAR.contains(*c)).collect();
    }
    if config.exclude_ambiguous {
        charset = charset.chars().filter(|c| !AMBIGUOUS.contains(*c)).collect();
    }

    if charset.is_empty() {
        return Err("No characters available after applying filters".to_string());
    }

    let charset_bytes = charset.as_bytes();
    let mut rng = rand::thread_rng();

    // Ensure at least one character from each required set
    let mut password = String::with_capacity(config.length);
    let mut required_chars = Vec::new();

    if config.lowercase {
        required_chars.push(pick_char(LOWERCASE, &mut rng));
    }
    if config.uppercase {
        required_chars.push(pick_char(UPPERCASE, &mut rng));
    }
    if config.digits {
        required_chars.push(pick_char(DIGITS, &mut rng));
    }
    if config.symbols {
        required_chars.push(pick_char(SYMBOLS, &mut rng));
    }

    // Fill remaining positions with random chars from the full charset
    while password.len() + required_chars.len() < config.length {
        let idx = rng.gen_range(0..charset_bytes.len());
        password.push(charset_bytes[idx] as char);
    }

    // Insert required chars at random positions
    for c in required_chars {
        let pos = if password.is_empty() {
            0
        } else {
            rng.gen_range(0..=password.len())
        };
        password.insert(pos, c);
    }

    Ok(password)
}

pub fn generate_passphrase(config: PassphraseConfig) -> Result<String, String> {
    let mut rng = rand::thread_rng();
    let word_count = config.word_count.max(3).min(20);

    let words: Vec<String> = (0..word_count)
        .map(|_| {
            let idx = rng.gen_range(0..DEFAULT_WORDLIST.len());
            let word = DEFAULT_WORDLIST[idx];

            match config.word_case.as_str() {
                "upper" => word.to_uppercase(),
                "title" => {
                    let mut c = word.chars();
                    match c.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                    }
                }
                _ => word.to_string(), // lower
            }
        })
        .collect();

    let mut passphrase = words.join(&config.separator);

    if config.include_number {
        let num: u16 = rng.gen_range(0..10000);
        passphrase.push_str(&num.to_string());
    }

    Ok(passphrase)
}

pub fn evaluate_strength(password: String) -> Result<StrengthResult, String> {
    let length = password.len();
    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_symbol = password.chars().any(|c| !c.is_ascii_alphanumeric());

    let _variety = [has_lower, has_upper, has_digit, has_symbol]
        .iter()
        .filter(|&&x| x)
        .count();

    // Calculate entropy roughly
    let mut pool_size = 0u32;
    if has_lower { pool_size += 26; }
    if has_upper { pool_size += 26; }
    if has_digit { pool_size += 10; }
    if has_symbol { pool_size += 33; }
    let entropy = length as f64 * (pool_size as f64).log2();

    // Score based on entropy and variety
    let score = if entropy < 28.0 {
        0
    } else if entropy < 36.0 {
        1
    } else if entropy < 60.0 {
        2
    } else if entropy < 80.0 {
        3
    } else if entropy < 100.0 {
        4
    } else {
        5
    };

    let (label, color) = match score {
        0 => ("Very Weak", "#DC2626"),
        1 => ("Weak", "#EF4444"),
        2 => ("Fair", "#F59E0B"),
        3 => ("Good", "#10B981"),
        4 => ("Strong", "#2563EB"),
        _ => ("Very Strong", "#7C3AED"),
    };

    // Estimate crack time (simplified: assume 10^10 guesses/second)
    let guesses_needed = 2f64.powf(entropy);
    let crack_time_seconds = guesses_needed / 10_000_000_000f64;

    let crack_time_display = if crack_time_seconds < 60.0 {
        "instantly".to_string()
    } else if crack_time_seconds < 3600.0 {
        format!("{} minutes", (crack_time_seconds / 60.0) as u32)
    } else if crack_time_seconds < 86400.0 {
        format!("{} hours", (crack_time_seconds / 3600.0) as u32)
    } else if crack_time_seconds < 31536000.0 {
        format!("{} days", (crack_time_seconds / 86400.0) as u32)
    } else if crack_time_seconds < 3153600000.0 {
        format!("{} years", (crack_time_seconds / 31536000.0) as u32)
    } else {
        "centuries".to_string()
    };

    let feedback = match score {
        0 => "Too short and predictable. Use at least 12 characters with mixed types.".to_string(),
        1 => "Weak. Add more characters and mix uppercase, digits, and symbols.".to_string(),
        2 => "Fair. Increase length and add more character variety.".to_string(),
        3 => "Good password. Could be stronger with more length.".to_string(),
        4 => "Strong password!".to_string(),
        _ => "Excellent password!".to_string(),
    };

    Ok(StrengthResult {
        score,
        label: label.to_string(),
        color: color.to_string(),
        feedback,
        crack_time_seconds,
        crack_time_display,
    })
}

fn pick_char(charset: &str, rng: &mut impl Rng) -> char {
    let bytes = charset.as_bytes();
    let idx = rng.gen_range(0..bytes.len());
    bytes[idx] as char
}
