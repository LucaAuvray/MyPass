'use strict';

/**
 * @Object kpxcSignup
 * Extracts the DOM facts that kpxcDetect.isSignupPasswordField needs.
 */
const kpxcSignup = {};

// passwordInputs: the field's group (kpxcPasswordGenerator.passwordGroup)
kpxcSignup.isSignupField = function(field, passwordInputs) {
    const tokens = (field.getAttribute('autocomplete') || '').toLowerCase().split(/\s+/);
    const autocomplete = tokens.includes('new-password') ? 'new-password'
        : tokens.includes('current-password') ? 'current-password' : '';

    const form = field.form;
    const submit = kpxcForm.getFormSubmitButton(form);
    const container = form || field.closest('main, section, [role="dialog"]') || document.body;

    return kpxcDetect.isSignupPasswordField({
        autocomplete,
        passwordCount: passwordInputs.length,
        index: passwordInputs.indexOf(field),
        buttonText: submit?.textContent || submit?.value || '',
        text: (container.textContent || '').slice(0, 2000),
        path: location.pathname
    });
};

/**
 * Menu under a sign-up password field offering one generated password.
 * Does not use Autocomplete.create(): it forces autocomplete="off", which would break signup detection.
 */
class PasswordSuggestion extends Autocomplete {
    constructor() {
        super();
        this.loading = false;
    }

    // Single listener set per field
    attach(field) {
        if (field.readOnly || this.autocompleteList.includes(field)) {
            return;
        }

        this.autocompleteList.push(field);
        field.addEventListener('focus', e => this.open(e));
        field.addEventListener('click', e => this.open(e));
        field.addEventListener('keydown', e => this.keyDown(e));
        field.addEventListener('keyup', e => this.keyUp(e));

        // Sign-up outside a <form> (kpxcForm.initForm covers forms): hook the submit buttons of the closest
        // ancestor, up to 4 levels, that has some
        let container = field.form ? null : field.parentElement;
        for (let level = 0; container && level < 4 && !kpxcForm.hookSubmitButtons(container); level++) {
            container = container.parentElement;
        }
    }

    async open(e) {
        const field = e.target;
        if (!e.isTrusted || this.loading || this.input === field || kpxc.databaseState !== DatabaseState.UNLOCKED) {
            return;
        }

        this.loading = true;
        try {
            const password = await sendMessage('generate_password');
            if (!password) {
                return;
            }

            this.elements = [{ title: tr('passwordSuggestionTitle'), value: password, group: tr('passwordSuggestionHint'), uuid: '' }];
            await this.showList(field);
        } finally {
            this.loading = false;
        }
    }

    // The list is one fixed item: no credential sorting or filtering
    async updateList() {
        while (this.list.hasChildNodes()) {
            this.list.removeChild(this.list.lastChild);
        }

        for (const c of this.elements) {
            const item = this.generateItem(c);
            item.addEventListener('click', e => this.itemClick(e, this.input, c.uuid));
            item.addEventListener('mousemove', e => this.mouseMove(e));
            item.addEventListener('mousedown', e => e.stopPropagation());
            item.addEventListener('mouseup', e => e.stopPropagation());
            this.list.appendChild(item);
        }

        this.selectItem();
    }

    async itemClick(e, input) {
        if (!e.isTrusted) {
            return;
        }

        e.stopPropagation();
        kpxcPasswordGenerator.fill(input, this.elements[0]?.value);
        this.clear();
        // Focus before closing: this.input is still set, so the focus handler does not reopen the menu
        input?.focus();
        this.closeList();
    }

    async itemEnter() {
        kpxcPasswordGenerator.fill(this.input, this.elements[0]?.value);
        this.clear();
    }
}

const kpxcPasswordSuggestion = new PasswordSuggestion();

document.addEventListener('click', e => {
    if (e.isTrusted) {
        handleOutsideClick(e, kpxcPasswordSuggestion);
    }
});
window.addEventListener('resize', () => kpxcPasswordSuggestion.updatePosition());
window.addEventListener('scroll', () => kpxcPasswordSuggestion.updatePosition());
