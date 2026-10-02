/**
 * Estimate password strength using heuristics.
 * Will be replaced by zxcvbn or Rust backend in production.
 */
export function estimateStrength(password: string): {
  score: number;
  label: string;
  color: string;
  feedback: string;
} {
  const length = password.length;
  const hasLower = /[a-z]/.test(password);
  const hasUpper = /[A-Z]/.test(password);
  const hasDigit = /[0-9]/.test(password);
  const hasSymbol = /[^a-zA-Z0-9]/.test(password);

  const variety = [hasLower, hasUpper, hasDigit, hasSymbol].filter(Boolean).length;

  let score = 0;
  if (length >= 8) score++;
  if (length >= 12) score++;
  if (length >= 16) score++;
  if (variety >= 2) score++;
  if (variety >= 3) score++;
  if (variety >= 4) score++;

  const labels = ["Very Weak", "Weak", "Fair", "Good", "Strong", "Very Strong"];
  const colors = ["#DC2626", "#EF4444", "#F59E0B", "#10B981", "#2563EB", "#7C3AED"];

  return {
    score: Math.min(score, 5),
    label: labels[Math.min(score, 5)],
    color: colors[Math.min(score, 5)],
    feedback: getFeedback(score, length, variety),
  };
}

function getFeedback(score: number, _length: number, _variety: number): string {
  if (score <= 1) return "Too short and predictable. Add more characters.";
  if (score === 2) return "Add uppercase letters, digits, and symbols.";
  if (score === 3) return "Good start. Make it longer and add more variety.";
  if (score === 4) return "Strong password! Consider making it slightly longer.";
  return "Excellent password!";
}
