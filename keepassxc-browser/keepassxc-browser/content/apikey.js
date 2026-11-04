'use strict';

// Tunables: adjust after surveying real API key consoles
const APIKEY_BATCH_MS = 300; // DOM additions are scanned in batches
const APIKEY_BATCH_MAX_CHARS = 100000; // text read per scan
const APIKEY_MAX_QUEUE = 500; // nodes kept per batch
const APIKEY_MAX_TEXT = 10000; // longer texts (inline JSON, logs) are skipped
const APIKEY_CONTEXT_MAX = 2000;
const APIKEY_CONTEXT_ANCESTORS = 4;
const APIKEY_DIALOG = '[role=dialog], dialog, [aria-modal=true]';
const APIKEY_DIALOG_RESCAN_MS = 500;
const APIKEY_DIALOG_RESCANS = 6; // ~3 s: the key often lands in an already mounted field after the network call
const APIKEY_UNLOCK_POLL_MS = 3000;
const APIKEY_MAX_CANDIDATES = 50; // per page: each one costs the local app 3 round trips
const APIKEY_SHAPE = /[\w.-]{19,}/; // cheap gate: a text without a long token costs nothing more

/**
 * @Object kpxcApiKey
 * Spots an API key shown or copied on the page and offers to save it in MyPass.
 * The key stays in this content script's memory; it only leaves the page in check_api_key / save_api_key
 * messages to the local MyPass app. Never log it.
 */
const kpxcApiKey = {};
kpxcApiKey.seen = new Set(); // keys already proposed on this page (capped by APIKEY_MAX_CANDIDATES)
kpxcApiKey.chain = Promise.resolve(); // offers run one at a time
kpxcApiKey.typed = new WeakSet(); // fields the user typed or pasted into: never proposed
kpxcApiKey.queue = new Set();
kpxcApiKey.dialogs = new Map(); // dialog -> rescans left
kpxcApiKey.clickContext = '';
kpxcApiKey.pending = undefined; // { key, url, hash } shown while the vault was locked

kpxcApiKey.init = function() {
    // The banner lives in the top document. shortcut: cross-origin frames are skipped, scan them if a console needs it.
    try {
        void window.top.document.body;
    } catch (_e) {
        return;
    }

    const userInput = (e) => e.isTrusted && kpxcApiKey.typed.add(e.composedPath()[0]);
    document.addEventListener('input', userInput, true);
    document.addEventListener('paste', userInput, true);

    // Context of a copy = around what was clicked last (the Copy button), read at click time: the button may be replaced
    const click = (e) => {
        const target = e.composedPath()[0];
        if (e.isTrusted && target instanceof Element) {
            kpxcApiKey.clickContext = kpxcApiKey.contextOf(target);
        }
    };
    document.addEventListener('pointerdown', click, true);
    document.addEventListener('click', click, true);

    document.addEventListener('copy', kpxcApiKey.onCopy, true);
    document.addEventListener('mypass-copy', (e) => kpxcApiKey.onCopied(String(e.detail ?? '')));

    new MutationObserver((mutations) => {
        for (const mut of mutations) {
            if (mut.type === 'characterData') {
                kpxcApiKey.enqueue(mut.target);
            } else {
                mut.addedNodes.forEach(kpxcApiKey.enqueue);
            }
        }
    }).observe(document.documentElement, { childList: true, subtree: true, characterData: true });

    kpxcApiKey.enqueue(document.body); // A key rendered by the server is already there
};

kpxcApiKey.enqueue = function(node) {
    if (node && kpxcApiKey.queue.size < APIKEY_MAX_QUEUE) {
        kpxcApiKey.queue.add(node);
        kpxcApiKey.batchTimer ??= setTimeout(kpxcApiKey.flush, APIKEY_BATCH_MS);
    }
};

kpxcApiKey.flush = function() {
    kpxcApiKey.batchTimer = undefined;
    const nodes = [ ...kpxcApiKey.queue ];
    kpxcApiKey.queue.clear();

    let budget = APIKEY_BATCH_MAX_CHARS;
    for (const node of nodes) {
        if (budget <= 0) {
            break;
        }
        budget -= kpxcApiKey.scan(node, budget);

        const el = node.nodeType === Node.ELEMENT_NODE ? node : node.parentElement;
        const dialog = el?.closest(APIKEY_DIALOG) ?? el?.querySelector(APIKEY_DIALOG);
        if (dialog) {
            kpxcApiKey.watchDialog(dialog);
        }
    }
};

// Rescans a dialog a few times: React sets a field's value property without any DOM mutation
kpxcApiKey.watchDialog = function(dialog) {
    const running = kpxcApiKey.dialogs.has(dialog);
    kpxcApiKey.dialogs.set(dialog, APIKEY_DIALOG_RESCANS);
    if (running) {
        return;
    }

    const timer = setInterval(() => {
        const left = kpxcApiKey.dialogs.get(dialog) - 1;
        kpxcApiKey.scan(dialog);
        if (left <= 0 || !dialog.isConnected) {
            clearInterval(timer);
            kpxcApiKey.dialogs.delete(dialog);
        } else {
            kpxcApiKey.dialogs.set(dialog, left);
        }
    }, APIKEY_DIALOG_RESCAN_MS);
};

// Examines the visible text and fields under node; returns the number of characters read
kpxcApiKey.scan = function(node, budget = APIKEY_BATCH_MAX_CHARS) {
    if (!node?.isConnected) {
        return 0;
    }
    if (node.nodeType === Node.TEXT_NODE) {
        return kpxcApiKey.check(node.data, node.parentElement);
    }
    if (node.nodeType !== Node.ELEMENT_NODE) {
        return 0;
    }

    let used = 0;
    const fields = node.matches('input, textarea') ? [ node ] : node.querySelectorAll('input, textarea');
    for (const field of fields) {
        if (!kpxcApiKey.typed.has(field)) {
            used += kpxcApiKey.check(field.value, field);
        }
    }

    const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
    while (used < budget && walker.nextNode()) {
        used += kpxcApiKey.check(walker.currentNode.data, walker.currentNode.parentElement);
    }
    return used;
};

// Proposes the first API key of a visible text; returns the characters read (the scan budget)
kpxcApiKey.check = function(text, el) {
    if (!text || text.length > APIKEY_MAX_TEXT) {
        return 0;
    }

    // Cheap gates first: the DOM is only read for a token that would be a key in a key-like context ("api key").
    // Editable regions hold what the user typed; checkVisibility rules out hidden inputs, display:none, opacity 0.
    if (APIKEY_SHAPE.test(text) && kpxcDetect.findApiKey(text, 'api key')
        && el && !el.isContentEditable && el.checkVisibility({ opacityProperty: true, visibilityProperty: true })) {
        const key = kpxcDetect.findApiKey(text, kpxcApiKey.contextOf(el));
        if (key) {
            kpxcApiKey.propose(key);
        }
    }
    return text.length;
};

// Text of the enclosing dialog, else of a few ancestors, joined with spaces (textContent glues "API key" to the next word)
kpxcApiKey.contextOf = function(el) {
    let root = el.closest(APIKEY_DIALOG);
    if (!root) {
        root = el;
        for (let i = 0; i < APIKEY_CONTEXT_ANCESTORS && root.parentElement; ++i) {
            root = root.parentElement;
        }
    }

    let text = '';
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    while (text.length < APIKEY_CONTEXT_MAX && walker.nextNode()) {
        text += walker.currentNode.data + ' ';
    }
    return text.slice(0, APIKEY_CONTEXT_MAX);
};

// Ctrl+C or a Copy button using execCommand: the selection, or the selected part of the focused field
kpxcApiKey.onCopy = function() {
    const field = document.activeElement;
    if (field?.isContentEditable) {
        return;
    }
    if (field?.matches('input, textarea')) {
        if (!kpxcApiKey.typed.has(field)) {
            kpxcApiKey.onCopied(field.value.slice(field.selectionStart ?? 0, field.selectionEnd ?? 0));
        }
        return;
    }
    kpxcApiKey.onCopied(document.getSelection()?.toString());
};

kpxcApiKey.onCopied = function(text) {
    if (text && text.length <= APIKEY_MAX_TEXT) {
        const key = kpxcDetect.findApiKey(text, kpxcApiKey.clickContext);
        if (key) {
            kpxcApiKey.propose(key);
        }
    }
};

kpxcApiKey.sha256 = async function(text) {
    const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
    return [ ...new Uint8Array(digest) ].map(b => b.toString(16).padStart(2, '0')).join('');
};

// Candidates are offered one at a time, at most APIKEY_MAX_CANDIDATES per page: a hostile page cannot flood the local app.
// crypto.subtle needs a secure context: API key consoles are all HTTPS.
kpxcApiKey.propose = function(key) {
    if (kpxcBanner.apiKey || kpxcApiKey.seen.has(key) || kpxcApiKey.seen.size >= APIKEY_MAX_CANDIDATES || !crypto.subtle) {
        return;
    }
    kpxcApiKey.seen.add(key);
    kpxcApiKey.chain = kpxcApiKey.chain.then(() => kpxcApiKey.offer(key));
};

// Already stored: nothing. Ignored this session: nothing. Locked: a banner anyway (the key may never show again).
kpxcApiKey.offer = async function(key) {
    if (kpxcBanner.apiKey) {
        return; // One banner at a time: the first key keeps it
    }

    try {
        const hash = await kpxcApiKey.sha256(key);
        if (await sendMessage('apikey_is_ignored', [ hash ])) {
            return;
        }

        // Asked first: it also refreshes the background's view of the vault state read just below
        if (await sendMessage('check_api_key', [ key ])) {
            return;
        }

        const status = await sendMessage('get_status', [ true ]);
        const locked = !status?.keePassXCAvailable || status.databaseClosed;
        const item = { key, url: window.top.location.href.split(/[?#]/)[0], hash };
        if (locked) {
            kpxcApiKey.pending ??= item; // One banner at a time: the first key keeps it
            kpxcApiKey.watchUnlock();
        }
        await kpxcBanner.createApiKey(item, locked);
    } catch (err) {
        logError('kpxcApiKey.offer error: ' + err);
    }
};

// The MyPass host does not push unlock events: poll while a locked banner waits
kpxcApiKey.watchUnlock = function() {
    kpxcApiKey.unlockTimer ??= setInterval(async () => {
        if (!kpxcApiKey.pending) {
            clearInterval(kpxcApiKey.unlockTimer);
            kpxcApiKey.unlockTimer = undefined;
        } else if (await sendMessage('get_database_hash')) {
            await kpxcApiKey.onUnlocked();
        }
    }, APIKEY_UNLOCK_POLL_MS);
};

// Vault unlocked: the locked banner closes if the key is already stored, else switches to Save
kpxcApiKey.onUnlocked = async function() {
    const item = kpxcApiKey.pending;
    if (!item) {
        return;
    }
    kpxcApiKey.pending = undefined;

    const exists = await sendMessage('check_api_key', [ item.key ]);
    if (kpxcBanner.apiKey !== item) {
        if (!exists) {
            await kpxcBanner.createApiKey(item, false);
        }
    } else if (exists) {
        await kpxcBanner.destroyApiKey();
    } else {
        kpxcBanner.unlockApiKey();
    }
};
