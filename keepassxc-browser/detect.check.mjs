// Runnable check for the extension's pure detectors: `node keepassxc-browser/detect.check.mjs`
import { readFileSync } from "node:fs";
import vm from "node:vm";
const src = readFileSync(new URL("./keepassxc-browser/content/detect.js", import.meta.url), "utf8");
const d = vm.runInNewContext(`${src}\n;kpxcDetect`, {});
let failed = 0;
const check = (ok, msg) => { console.log(`${ok ? "ok  " : "FAIL"} - ${msg}`); if (!ok) failed++; };

const MISTRAL = "Xq7bR2kLm9TzP4wV8nC1sD6fG3hJ5aYe";
const CTX = "Your API key. Make sure to copy it now, you won't be able to see it again.";
check(d.isApiKeyToken("sk-proj-" + "a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8s9T0", ""), "OpenAI project key without context");
check(d.isApiKeyToken("ghp_" + "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8", ""), "GitHub token without context");
check(d.isApiKeyToken(MISTRAL, CTX), "prefix-less Mistral key inside an API key dialog");
check(!d.isApiKeyToken(MISTRAL, "Order reference"), "same string without key context");
check(d.isApiKeyToken(MISTRAL, "API keys — Create new key") && d.isApiKeyToken(MISTRAL, "Personal access tokens"), "plural key context");
check(!d.isApiKeyToken("3f786850e387550fdab836ed7e6dc881de23001b", "token"), "40-hex commit SHA");
check(!d.isApiKeyToken("123e4567-e89b-12d3-a456-426614174000", "API key"), "UUID");
check(!d.isApiKeyToken("averyveryverylongwordwithoutanydigits", "API key"), "no digit");
check(!d.isApiKeyToken("sk-abc12", ""), "prefix too short");
check(!d.isApiKeyToken("sk-circle-notification-banner-large", "") && !d.isApiKeyToken("hf_get_model_config_for_task", ""), "prefixed slugs");
check(d.isApiKeyToken("AKIAIOSFODNN7EXAMPLE", ""), "20-char AWS key ID");
check(d.findApiKey(`Your new key: ${MISTRAL} Copy`, CTX) === MISTRAL, "findApiKey extracts the key from text");
check(d.findApiKey("Welcome to the console", CTX) === null, "findApiKey on plain text");

const f = (o) => d.isSignupPasswordField({ autocomplete: "", passwordCount: 1, index: 0, buttonText: "", text: "", path: "/", ...o });
check(f({ autocomplete: "new-password" }), "new-password");
check(!f({ autocomplete: "current-password", passwordCount: 2 }), "current-password wins");
check(f({ passwordCount: 2, index: 0 }) && f({ passwordCount: 2, index: 1 }), "password + confirm");
check(!f({ passwordCount: 3, index: 0 }) && f({ passwordCount: 3, index: 1 }), "change form: current excluded");
check(!f({ buttonText: "Sign in", text: "Don't have an account? Sign up" }), "login page with a Sign up link");
check(!f({ buttonText: "Continue", text: "Welcome back. Don't have an account? Sign up" }), "ambiguous login page");
check(f({ buttonText: "Créer un compte" }), "French signup button");
check(f({ path: "/signup" }) && !f({ path: "/login" }), "signup / login paths");
check(f({ text: "Create your account" }), "signup heading only");
check(!f({ buttonText: "Continue", text: "Enter your password. Don't have an account? Sign up" }), "login page with Continue + Sign up link");
check(!f({ text: "Get design updates by email" }) && !f({ text: "Registered trademark of Acme" }), "signup words only as whole words");

check(d.isSubmitText("Create account") && d.isSubmitText("Continuer"), "submit words");
check(!d.isSubmitText("Forgot password?"), "non-submit link");
check(!d.isSubmitText("Discontinue") && !d.isSubmitText("Joint account"), "submit words only as whole words");

console.log(failed ? `\n${failed} check(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
