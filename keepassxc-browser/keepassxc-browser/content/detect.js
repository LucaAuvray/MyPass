'use strict';

/**
 * @Object kpxcDetect
 * Pure detectors (no DOM, no browser API) for signup password fields and API keys.
 * Checked by keepassxc-browser/detect.check.mjs.
 */
const kpxcDetect = {};

kpxcDetect.KEY_PREFIX = /^(?:sk-|sk-proj-|sk-ant-|ghp_|gho_|github_pat_|glpat-|hf_|gsk_|xoxb-|xoxp-|AIza|AKIA|sk_live_|rk_live_|sk_test_)[A-Za-z0-9_-]{20,}$/;
kpxcDetect.KEY_CONTEXT = ['api key', 'api-key', 'apikey', 'clé api', 'clé d\'api', 'secret', 'token', 'jeton', 'access key',
    'copy it now', 'won\'t be able to see', 'ne sera plus affichée'];
kpxcDetect.SIGNUP_WORDS = ['sign up', 'signup', 'register', 'create account', 'create an account', 'create your account', 'join',
    'inscription', 's\'inscrire', 'créer un compte', 'créer mon compte'];
kpxcDetect.LOGIN_WORDS = ['sign in', 'log in', 'login', 'welcome back', 'connexion', 'se connecter'];
kpxcDetect.SUBMIT_WORDS = ['continue', 'next', 'submit', 'valider', 'continuer', 'suivant', 'envoyer'];

kpxcDetect.hasAny = function(text, words) {
    return words.some(w => text.includes(w));
};

// Shannon entropy in bits per character
kpxcDetect.entropy = function(s) {
    const counts = {};
    for (const c of s) {
        counts[c] = (counts[c] || 0) + 1;
    }
    return -Object.values(counts).reduce((sum, n) => sum + (n / s.length) * Math.log2(n / s.length), 0);
};

kpxcDetect.isApiKeyToken = function(token, context) {
    // Known vendor prefix: no context needed
    if (kpxcDetect.KEY_PREFIX.test(token)) {
        return true;
    }

    // Prefix-less: needs a key-like shape and a key-like context
    return /^[A-Za-z0-9_.-]{24,200}$/.test(token)
        && /[A-Za-z]/.test(token) && /\d/.test(token)
        && !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(token)
        && !/^(?:[0-9a-f]{40}|[0-9a-f]{64})$/i.test(token)
        && kpxcDetect.entropy(token) >= 3.5
        && kpxcDetect.hasAny((context || '').toLowerCase(), kpxcDetect.KEY_CONTEXT);
};

// First whitespace/quote-separated token of the text that looks like an API key
kpxcDetect.findApiKey = function(text, context) {
    const tokens = (text || '').split(/[\s"'`“”‘’]+/).map(t => t.replace(/^[.,;:()[\]]+|[.,;:()[\]]+$/g, ''));
    return tokens.find(t => kpxcDetect.isApiKeyToken(t, context)) || null;
};

// Rules are checked in order; the first one that decides wins
kpxcDetect.isSignupPasswordField = function({ autocomplete, passwordCount, index, buttonText, text, path }) {
    if (autocomplete === 'current-password') {
        return false;
    }
    if (autocomplete === 'new-password') {
        return true;
    }
    if (passwordCount >= 2) {
        // With three fields (change form) the first one is the current password
        return !(passwordCount >= 3 && index === 0);
    }

    const button = (buttonText || '').toLowerCase();
    if (kpxcDetect.hasAny(button, kpxcDetect.SIGNUP_WORDS)) {
        return true;
    }
    if (kpxcDetect.hasAny(button, kpxcDetect.LOGIN_WORDS)) {
        return false;
    }

    const p = (path || '').toLowerCase();
    if (/\/signup|\/sign-up|\/register|\/join|\/inscription|\/create-account/.test(p)) {
        return true;
    }
    if (/\/login|\/signin|\/sign-in|\/connexion/.test(p)) {
        return false;
    }

    // Page text is noisy (a login page may link to "Sign up"): require signup words and no login words
    const t = (text || '').toLowerCase();
    return kpxcDetect.hasAny(t, kpxcDetect.SIGNUP_WORDS) && !kpxcDetect.hasAny(t, kpxcDetect.LOGIN_WORDS);
};

// Short button/link text that submits a signup step
kpxcDetect.isSubmitText = function(text) {
    const t = (text || '').trim().toLowerCase();
    return t.length > 0 && t.length <= 40
        && (kpxcDetect.hasAny(t, kpxcDetect.SIGNUP_WORDS) || kpxcDetect.hasAny(t, kpxcDetect.SUBMIT_WORDS));
};
