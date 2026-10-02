'use strict';

// MyPass : remplissage d'identités et de cartes bancaires (absent de
// l'upstream keepassxc-browser). Détecte les champs via l'attribut
// autocomplete, puis par heuristiques name/id/placeholder/label (FR+EN).

const kpxcIdentityFill = {};
kpxcIdentityFill.icons = [];

// Jeton autocomplete -> clé MyPass ('@x' = valeur composée, null = ignoré)
kpxcIdentityFill.autocompleteMap = {
    'cc-number': 'CC_Number',
    'cc-name': 'CC_Holder',
    'cc-csc': 'CC_CVC',
    'cc-exp': '@cc-exp',
    'cc-exp-month': 'CC_ExpMonth',
    'cc-exp-year': 'CC_ExpYear',
    'given-name': 'ID_FirstName',
    'family-name': 'ID_LastName',
    'name': '@full-name',
    'email': 'ID_Email',
    'tel': 'ID_Phone',
    'tel-national': 'ID_Phone',
    'street-address': 'ID_Address',
    'address-line1': 'ID_Address',
    'address-level2': 'ID_City',
    'postal-code': 'ID_PostalCode',
    'country': 'ID_Country',
    'country-name': 'ID_Country',
    'bday': 'ID_BirthDate',
    'organization': 'ID_Company'
};

// Heuristiques de repli, testées dans l'ordre (carte avant identité :
// « nom sur la carte » doit matcher CC_Holder, pas ID_LastName).
kpxcIdentityFill.heuristics = [
    [ /card.?number|cardnum|num[eé]ro.?(de.?)?carte|num.?cb|\bpan\b/i, 'CC_Number' ],
    [ /cvc|cvv|csc|cryptogramme|security.?code/i, 'CC_CVC' ],
    [ /exp.*month|mois.*exp/i, 'CC_ExpMonth' ],
    [ /exp.*year|ann[eé]e.*exp/i, 'CC_ExpYear' ],
    [ /expir/i, '@cc-exp' ],
    [ /card.?holder|titulaire|name.?on.?card|nom.*carte/i, 'CC_Holder' ],
    [ /first.?name|pr[eé]nom|given/i, 'ID_FirstName' ],
    // \bnom\b (pas ^nom$ : le haystack est concaténé) ; « prénom » est
    // capté avant par ID_FirstName (pr[eé]nom), l'ordre protège du faux positif.
    [ /last.?name|surname|family|nom.?de.?famille|\bnom\b/i, 'ID_LastName' ],
    [ /e.?mail|courriel/i, 'ID_Email' ],
    [ /phone|mobile|t[eé]l[eé]phone|portable/i, 'ID_Phone' ],
    [ /postal.?code|\bzip\b|code.?postal/i, 'ID_PostalCode' ],
    [ /address|adresse|\brue\b|street/i, 'ID_Address' ],
    [ /city|ville|town/i, 'ID_City' ],
    [ /country|pays/i, 'ID_Country' ],
    [ /birth|naissance/i, 'ID_BirthDate' ],
    [ /company|soci[eé]t[eé]|organi[sz]ation/i, 'ID_Company' ]
];

kpxcIdentityFill.labelText = function(field) {
    return field.labels?.[0]?.textContent ?? '';
};

// Clé MyPass d'un champ, ou null s'il ne nous concerne pas
kpxcIdentityFill.fieldKey = function(field) {
    if (field.type === 'password' || field.type === 'hidden' || field.readOnly) {
        return null;
    }

    const auto = field.getLowerCaseAttribute('autocomplete');
    if (auto) {
        for (const token of auto.split(' ')) {
            if (token in kpxcIdentityFill.autocompleteMap) {
                return kpxcIdentityFill.autocompleteMap[token];
            }
        }
    }

    const haystack = [ field.name, field.id, field.placeholder, kpxcIdentityFill.labelText(field) ]
        .filter(Boolean).join(' ');
    for (const [ regex, key ] of kpxcIdentityFill.heuristics) {
        if (regex.test(haystack)) {
            return key;
        }
    }

    return null;
};

// Scanne la page, pose une icône par formulaire identité/carte détecté
kpxcIdentityFill.scan = function() {
    const byRoot = new Map(); // form (ou document) -> [{ el, key }]
    for (const el of document.querySelectorAll('input, select')) {
        if (el.offsetParent === null) {
            continue; // champ invisible
        }
        const key = kpxcIdentityFill.fieldKey(el);
        if (!key) {
            continue;
        }
        const root = el.form || document;
        if (!byRoot.has(root)) {
            byRoot.set(root, []);
        }
        byRoot.get(root).push({ el: el, key: key });
    }

    for (const fields of byRoot.values()) {
        const isPayment = fields.some(f => f.key === 'CC_Number' || f.key === 'CC_CVC' || f.key === '@cc-exp');
        const identityCount = fields.filter(f => f.key.startsWith('ID_') || f.key === '@full-name').length;
        // Un formulaire de login avec juste un email n'est pas un formulaire d'identité
        if (!isPayment && identityCount < 2) {
            continue;
        }

        const anchor = fields[0].el;
        if (anchor.getAttribute('kpxc-identity-icon') === '1') {
            continue;
        }
        anchor.setAttribute('kpxc-identity-icon', '1');

        const formEntry = { fields: fields, type: isPayment ? 'card' : 'identity' };
        kpxcIdentityFill.icons.push(new IdentityFieldIcon(anchor, formEntry, kpxc.databaseState));
    }
};

kpxcIdentityFill.showMenu = async function(field, formEntry) {
    const all = await sendMessage('get_identities');
    const items = (all || []).filter(i => i.type === formEntry.type);
    if (items.length === 0) {
        showErrorNotification(formEntry.type === 'card'
            ? 'MyPass : aucune carte dans le coffre'
            : 'MyPass : aucune identité dans le coffre', 'warning');
        return;
    }

    if (items.length === 1) {
        await kpxcIdentityFill.fill(formEntry, items[0]);
        return;
    }

    kpxcIdentityAutocomplete.identityItems = items;
    kpxcIdentityAutocomplete.formEntry = formEntry;
    kpxcIdentityAutocomplete.elements = items.map(i => ({
        title: i.title,
        value: i.type === 'card'
            ? ('•••• ' + (i.fields.CC_Number || '').slice(-4))
            : (i.fields.ID_Email || [ i.fields.ID_FirstName, i.fields.ID_LastName ].filter(Boolean).join(' ')),
        group: '',
        uuid: i.uuid
    }));
    kpxcIdentityAutocomplete.showList(field, true);
};

// Valeur à écrire pour une clé, gère les valeurs composées
kpxcIdentityFill.resolveValue = function(key, fields) {
    if (key === '@cc-exp') {
        const month = (fields.CC_ExpMonth || '').padStart(2, '0');
        const year = (fields.CC_ExpYear || '').slice(-2);
        return (month !== '00' && year) ? `${month}/${year}` : '';
    }
    if (key === '@full-name') {
        return [ fields.ID_FirstName, fields.ID_LastName ].filter(Boolean).join(' ');
    }
    return fields[key] || '';
};

kpxcIdentityFill.fill = async function(formEntry, item) {
    for (const { el, key } of formEntry.fields) {
        const value = kpxcIdentityFill.resolveValue(key, item.fields);
        if (!value) {
            continue;
        }
        if (el instanceof HTMLSelectElement) {
            kpxcIdentityFill.fillSelect(el, key, value);
        } else {
            await kpxcFill.setValue(el, value);
        }
    }
};

// Les expirations sont souvent des <select> : "1"/"01"/"2027"...
kpxcIdentityFill.fillSelect = function(el, key, value) {
    const candidates = [ value ];
    if (key === 'CC_ExpMonth') {
        candidates.push(String(Number(value)), value.padStart(2, '0'));
    } else if (key === 'CC_ExpYear') {
        candidates.push(value.length === 2 ? '20' + value : value.slice(-2));
    }
    const option = Array.from(el.options).find(o =>
        candidates.includes(o.value.trim()) || candidates.includes(o.textContent.trim()));
    if (option) {
        el.value = option.value;
        el.dispatchEvent(new Event('change', { bubbles: true }));
    }
};

class IdentityFieldIcon extends Icon {
    constructor(field, formEntry, databaseState = DatabaseState.DISCONNECTED) {
        super(field, databaseState);
        this.formEntry = formEntry;
        this.createIcon(field);
        this.inputField = field;
        kpxcIcons.monitorIconPosition(this);
    }
}

// Même construction que TOTPFieldIcon, en réutilisant le style de
// l'icône username (pas de nouveau CSS). L'image de fond vit sur les
// classes d'état (.unlock/.lock/.disconnected), pas sur .kpxc-username-icon
// seule — on dérive donc la classe d'état comme username-field.js.
IdentityFieldIcon.prototype.createIcon = function(field) {
    const stateClass = this.databaseState === DatabaseState.LOCKED
        ? getIconClass('lock')
        : this.databaseState === DatabaseState.DISCONNECTED
            ? getIconClass('disconnected')
            : getIconClass('unlock');
    const size = this.calculateIconSize(field);
    const formEntry = this.formEntry;

    const icon = kpxcUI.createElement('div', 'kpxc kpxc-username-icon ' + stateClass,
        {
            'title': 'MyPass : remplir avec une identité / carte enregistrée',
            'size': size,
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
    } else {
        icon.style.filter = 'saturate(100%)';
    }

    icon.addEventListener('click', async function(e) {
        if (!e.isTrusted) {
            return;
        }
        e.stopPropagation();
        await kpxcIdentityFill.showMenu(field, formEntry);
    });

    icon.addEventListener('mousedown', ev => ev.stopPropagation());
    icon.addEventListener('mouseup', ev => ev.stopPropagation());

    kpxcIcons.setIconPosition(icon, field, this.rtl);
    this.icon = icon;
    this.createWrapper('css/username.css');
    if (kpxcFields.popoverSupported) {
        icon.showPopover();
    }
};

class IdentityAutocomplete extends Autocomplete {}

IdentityAutocomplete.prototype.itemClick = async function(e, input, uuid) {
    if (!e.isTrusted) {
        return;
    }
    const item = this.identityItems.find(i => i.uuid === uuid);
    if (item) {
        await kpxcIdentityFill.fill(this.formEntry, item);
    }
    this.closeList();
};

IdentityAutocomplete.prototype.itemEnter = async function(index, item) {
    const uuid = item?.getAttribute('uuid');
    const found = this.identityItems.find(i => i.uuid === uuid);
    if (found) {
        await kpxcIdentityFill.fill(this.formEntry, found);
    }
};

const kpxcIdentityAutocomplete = new IdentityAutocomplete();

// Scan initial + re-scan sur mutations (checkouts SPA), avec debounce.
(function initIdentityFill() {
    let timer;
    const rescan = () => {
        clearTimeout(timer);
        timer = setTimeout(() => kpxcIdentityFill.scan(), 500);
    };

    if (document.readyState !== 'loading') {
        rescan();
    } else {
        document.addEventListener('DOMContentLoaded', rescan);
    }

    // ponytail: MutationObserver global débounce à 500 ms ; si un site
    // très dynamique pose problème, brancher sur observer-helper.js
    new MutationObserver(rescan).observe(document.documentElement, { childList: true, subtree: true });

    // Fermeture du menu : l'icône et les items du menu font stopPropagation
    // sur mousedown, donc la sélection n'est pas cassée. closeList() est
    // inoffensif si la liste n'est jamais ouverte (guard !shadowRoot).
    document.addEventListener('mousedown', () => kpxcIdentityAutocomplete.closeList());
    document.addEventListener('keydown', e => {
        if (e.key === 'Escape') {
            kpxcIdentityAutocomplete.closeList();
        }
    });
})();
