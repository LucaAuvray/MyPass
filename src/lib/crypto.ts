/**
 * Frontend crypto utilities for MyPass.
 * Heavy crypto operations are delegated to the Rust backend,
 * but lightweight helpers and typed wrappers live here.
 */

import { tauriCommand } from "./tauri";

/** Result of a password strength evaluation */
export interface StrengthResult {
  score: number;       // 0-5
  label: string;       // "Very Weak" to "Very Strong"
  color: string;       // hex color
  feedback: string;    // human-readable advice
  crackTimeSeconds: number;
  crackTimeDisplay: string;
}

/** Configuration for password generation */
export interface PasswordConfig {
  length: number;
  uppercase: boolean;
  lowercase: boolean;
  digits: boolean;
  symbols: boolean;
  excludeSimilar: boolean;
  excludeAmbiguous: boolean;
}

/** Configuration for passphrase generation */
export interface PassphraseConfig {
  wordCount: number;
  separator: string;
  wordCase: "lower" | "upper" | "title";
  includeNumber: boolean;
}

// =============================================================================
// Tauri-backed crypto commands
// =============================================================================

/** Generate a random password using CSPRNG from the Rust backend */
export async function generatePassword(config: PasswordConfig): Promise<string> {
  return tauriCommand<string>("generate_password", { config });
}

/** Generate a random passphrase from the Rust backend */
export async function generatePassphrase(config: PassphraseConfig): Promise<string> {
  return tauriCommand<string>("generate_passphrase", { config });
}

/** Evaluate password strength using the Rust backend (zxcvbn or equivalent) */
export async function evaluateStrength(password: string): Promise<StrengthResult> {
  return tauriCommand<StrengthResult>("evaluate_strength", { password });
}

/** Check if a password appears in known breaches (HIBP k-anonymity) */
export async function checkHibp(prefix: string): Promise<string[]> {
  return tauriCommand<string[]>("check_hibp", { prefix });
}
