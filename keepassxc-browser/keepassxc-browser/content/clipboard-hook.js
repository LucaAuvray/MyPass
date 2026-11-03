'use strict';

// Runs in the page's own world (manifest "world": "MAIN"), so no extension API here.
// Tells the content script (content/apikey.js) what a JavaScript "Copy" button writes to the clipboard.
(() => {
    const clipboard = navigator.clipboard;
    if (!clipboard?.writeText) {
        return;
    }

    const writeText = clipboard.writeText;
    clipboard.writeText = function(text) {
        document.dispatchEvent(new CustomEvent('mypass-copy', { detail: String(text) }));
        return writeText.apply(this, arguments);
    };
})();
