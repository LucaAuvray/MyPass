'use strict';

const kpxcPasswordIcons = {};
kpxcPasswordIcons.icons = [];

kpxcPasswordIcons.newIcon = function(field, databaseState = DatabaseState.DISCONNECTED) {
    kpxcPasswordIcons.icons.push(new PasswordIcon(field, databaseState));
};

kpxcPasswordIcons.switchIcon = function(state) {
    kpxcPasswordIcons.icons.forEach(u => u.switchIcon(state));
};

kpxcPasswordIcons.isValid = function(field) {
    if (!field
        || field.readOnly
        || field.offsetWidth < MIN_INPUT_FIELD_OFFSET_WIDTH
        || kpxcIcons.hasIcon(field)
        || !kpxcFields.isVisible(field)) {
        return false;
    }

    return true;
};


class PasswordIcon extends Icon {
    constructor(field, databaseState = DatabaseState.DISCONNECTED) {
        super(field, databaseState);
        this.nextFieldExists = false;

        this.initField(field);
        kpxcIcons.monitorIconPosition(this);
    }
}

PasswordIcon.prototype.initField = function(field) {
    // Observer the visibility
    if (this.observer) {
        this.observer.observe(field);
    }

    this.createIcon(field);
    this.inputField = field;
};

PasswordIcon.prototype.createIcon = function(field) {
    const className = getIconClass('key');
    const size = this.calculateIconSize(field);

    const icon = kpxcUI.createElement('div', 'kpxc kpxc-pwgen-icon ' + className,
        {
            'title': tr('passwordGeneratorGenerateText'),
            'size': size,
            'kpxc-pwgen-field-id': field.getAttribute('data-kpxc-id'),
            'popover': 'manual'
        });

    if (kpxcFields.popoverSupported) {
        icon.style.margin = 0;
    } else {
        icon.style.zIndex = '10000000';
    }
    icon.style.width = Pixels(size);
    icon.style.height = Pixels(size);

    if (this.databaseState === DatabaseState.DISCONNECTED || this.databaseState === DatabaseState.LOCKED) {
        icon.style.filter = 'saturate(0%)';
    }

    icon.addEventListener('click', async function(e) {
        if (!e.isTrusted) {
            return;
        }

        if (e.shiftKey) {
            if (kpxcFields.popoverSupported) {
                icon.hidePopover();
            } else {
                icon.style.display = 'none';
            }
            return;
        }

        e.stopPropagation();
        kpxcPasswordGenerator.showPasswordGenerator(field);
    });

    icon.addEventListener('mousedown', ev => ev.stopPropagation());
    icon.addEventListener('mouseup', ev => ev.stopPropagation());

    kpxcIcons.setIconPosition(icon, field, this.rtl);
    this.icon = icon;
    this.createWrapper('css/pwgen.css');
    if (kpxcFields.popoverSupported) {
        icon.showPopover();
    }
};


const kpxcPasswordGenerator = {};

kpxcPasswordGenerator.showPasswordGenerator = async function(field) {
    kpxcPasswordGenerator.generate(field ?? document.activeElement);
};

kpxcPasswordGenerator.generate = async function(field) {
    if (!await isPasswordGeneratorSupported()) {
        kpxcUI.createNotification('error', tr('passwordGeneratorNotSupported'));
        return;
    }

    kpxcPasswordGenerator.fill(field, await sendMessage('generate_password'));
};

kpxcPasswordGenerator.fill = async function(elem, password) {
    if (!elem || !password) {
        return;
    }

    if (password.length === 0) {
        kpxcUI.createNotification('error', tr('usernameLockedFieldText'));
        return;
    }

    if (elem.getAttribute('maxlength')) {
        if (password.length > elem.getAttribute('maxlength')) {
            const message =
                tr('passwordGeneratorErrorTooLong') +
                '\r\n' +
                tr('passwordGeneratorErrorTooLongCut') +
                '\r\n' +
                tr('passwordGeneratorErrorTooLongRemember');
            kpxcUI.createNotification('error', message);
            return;
        }
    }

    setNativeValue(elem, password);
    elem.dispatchEvent(new Event('input', { bubbles: true }));
    elem.dispatchEvent(new Event('change', { bubbles: true }));

    // Fill the confirmation field if found
    const nextField = kpxcPasswordGenerator.nextPasswordField(elem);
    if (nextField) {
        setNativeValue(nextField, password);
        nextField.dispatchEvent(new Event('input', { bubbles: true }));
        nextField.dispatchEvent(new Event('change', { bubbles: true }));
    }

    // Remember the generated password for this tab: the save banner follows the sign-up across pages.
    // On this page it waits for the submit (set before the message: a retrieval may read the pending save meanwhile).
    kpxcPasswordGenerator.awaitingSubmit = password;
    const combination = kpxc.combinations.find(c => c.password === elem || c.passwordInputs?.includes(elem));
    const url = trimURL(kpxc.settings.saveDomainOnlyNewCreds ? window.top.location.origin : window.top.location.href);
    await sendMessage('page_set_submitted', [ true, combination?.username?.value ?? '', password, url, kpxc.credentials, true ]);

    // The username may be typed after the fill: re-read it when leaving the page
    const usernameField = combination?.username;
    if (usernameField && window.self === window.top) {
        window.addEventListener('beforeunload', () => {
            if (usernameField.value) {
                sendMessage('page_update_submitted_username', [ usernameField.value, password ]);
            }
        });
    }
};

// The visible password fields grouped with elem (always included), in document order: its form's, else those of the
// closest ancestor (up to 6 levels) holding two or more. Read from the DOM, not kpxc.inputs: detection skips or drops
// a field covered at that time (the suggestion menu), and outside a <form> each field is its own combination.
// checkVisibility, not kpxcFields.isVisible: the menu covers the confirmation field while fill runs.
kpxcPasswordGenerator.passwordGroup = function(elem) {
    let scope = elem.form;
    let node = elem.parentNode;
    for (let level = 0; !scope && node?.querySelectorAll && level < 6; level++, node = node.parentNode) {
        if (node.querySelectorAll('input[type=password]').length >= 2) {
            scope = node;
        }
    }

    return [ ...(scope?.querySelectorAll('input[type=password]') ?? [ elem ]) ]
        .filter(f => f === elem || f.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }));
};

// The next usable password field after elem in its group (the confirmation to fill)
kpxcPasswordGenerator.nextPasswordField = function(elem) {
    return kpxcPasswordGenerator.passwordGroup(elem).find(f =>
        elem.compareDocumentPosition(f) & Node.DOCUMENT_POSITION_FOLLOWING
        && !f.disabled
        && !f.readOnly
        && !f.getAttribute('autocomplete')?.includes('current-password'));
};

const isPasswordGeneratorSupported = async function() {
    const response = await browser.runtime.sendMessage({
        action: 'get_features_list'
    });
    return response?.passwordGenerator;
};
