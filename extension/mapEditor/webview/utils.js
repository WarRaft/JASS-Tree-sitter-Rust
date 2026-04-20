'use strict';

// ── Shared utility functions ────────────────────────────────────────
// Populates window._W3E_UTILS for use by other webview modules.

window._W3E_UTILS = (function () {

    function esc(s) {
        return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
    }

    function indexToRgb(index) {
        const golden = 137.508;
        const hue = (index * golden) % 360;
        const sat = 0.55 + 0.15 * ((index % 3) / 2);
        const lum = 0.45 + 0.10 * ((index % 5) / 4);
        const cc = (1 - Math.abs(2 * lum - 1)) * sat;
        const xx = cc * (1 - Math.abs((hue / 60 % 2) - 1));
        const mm = lum - cc / 2;
        let r, g, b;
        if (hue < 60) { r = cc; g = xx; b = 0; }
        else if (hue < 120) { r = xx; g = cc; b = 0; }
        else if (hue < 180) { r = 0; g = cc; b = xx; }
        else if (hue < 240) { r = 0; g = xx; b = cc; }
        else if (hue < 300) { r = xx; g = 0; b = cc; }
        else { r = cc; g = 0; b = xx; }
        return [Math.round((r + mm) * 255), Math.round((g + mm) * 255), Math.round((b + mm) * 255)];
    }

    // ── WESTRING resolution ─────────────────────────────────────
    let _westringsMap = {};

    function setWestrings(map) {
        _westringsMap = (map && typeof map === 'object') ? map : {};
    }

    function resolveWestring(val) {
        if (!val || typeof val !== 'string') return val || '';
        let current = val;
        for (let i = 0; i < 3; i++) {
            if (!current.startsWith('WESTRING_')) break;
            let resolved = _westringsMap[current];
            if (resolved === undefined) break;
            current = resolved;
        }
        return current;
    }

    // ── GameString helpers ──────────────────────────────────────
    function gsValue(gs) {
        if (!gs) return '';
        if (typeof gs === 'object' && gs.value !== undefined) return gs.value;
        return String(gs);
    }

    function gsHtml(gs) {
        if (!gs) return '';
        if (typeof gs === 'object' && gs.value !== undefined) {
            let v = esc(gs.value);
            if (gs.original && gs.original !== gs.value) {
                return '<a href="#" class="gs-resolved" data-gs-original="' + esc(gs.original) + '" data-gs-source="' + esc(gs.source || '') + '">' + v + '</a>';
            }
            return v;
        }
        return esc(String(gs));
    }

    function _showGameStringInfo(value, original, source) {
        let win = document.getElementById('gameStringInfoWindow');
        if (!win) return;
        let body = win.querySelector('#gsInfoBody');
        if (!body) return;

        // Detect TRIGSTR source with line info: "war3map.wts:42"
        let sourceHtml;
        const wtsMatch = source.match(/^(.+\.wts):(\d+)$/);
        if (wtsMatch) {
            const wtsFile = wtsMatch[1];
            const wtsLine = parseInt(wtsMatch[2], 10);
            sourceHtml = '<a href="#" class="gs-wts-link" data-wts-file="' + esc(wtsFile) + '" data-wts-line="' + wtsLine + '">' + esc(wtsFile) + ':' + (wtsLine + 1) + '</a>';
        } else {
            sourceHtml = esc(source);
        }

        body.innerHTML =
            '<table class="info">' +
            '<tr><td class="key">value</td><td>' + esc(value) + '</td></tr>' +
            '<tr><td class="key">original</td><td><span class="code">' + esc(original) + '</span></td></tr>' +
            '<tr><td class="key">source</td><td>' + sourceHtml + '</td></tr>' +
            '</table>';
        win.setAttribute('title-text', '\ud83d\udd17 ' + value);
        win.show();
    }

    document.addEventListener('click', function (e) {
        // Handle click on WTS navigation link
        let wtsLink = e.target.closest('.gs-wts-link');
        if (wtsLink) {
            e.preventDefault();
            const file = wtsLink.getAttribute('data-wts-file') || 'war3map.wts';
            const line = parseInt(wtsLink.getAttribute('data-wts-line') || '0', 10);
            if (typeof vscode !== 'undefined') {
                vscode.postMessage({command: 'openFile', name: file, line: line});
            }
            return;
        }

        let link = e.target.closest('.gs-resolved');
        if (!link) return;
        e.preventDefault();
        let original = link.getAttribute('data-gs-original') || '';
        let source = link.getAttribute('data-gs-source') || '';
        let value = link.textContent || '';
        _showGameStringInfo(value, original, source);
    });

    // ── Detail view shared helpers ──────────────────────────────
    function colorBadge(r, g, b) {
        return '<span class="dd-color-badge" style="background:rgb(' + r + ',' + g + ',' + b + ')" title="rgb(' + r + ',' + g + ',' + b + ')"></span>';
    }

    function categoryBadge(code, categoriesMap) {
        const label = categoriesMap[code] || code;
        return '<span class="ds-ts-badge">' + esc(code) + '</span> ' + esc(label);
    }

    function tilesetBadges(val) {
        if (val === '*') {
            return '<span class="ds-ts-badge" style="background:rgba(78,154,241,0.25);color:var(--vscode-textLink-foreground,#3794ff);">*</span> All';
        }
        const chars = val.replace(/,/g, '').split('');
        return chars.map(function (ch) {
            const label = TILESET_NAMES[ch] || ch;
            return '<span class="ds-ts-badge" title="' + esc(label) + '">' + esc(ch) + '</span>';
        }).join(' ');
    }

    function buildModelPaths(filePath, numVar) {
        const lastSlash = Math.max(filePath.lastIndexOf('/'), filePath.lastIndexOf('\\'));
        const dotIdx = filePath.lastIndexOf('.');
        const hasExt = dotIdx > lastSlash && dotIdx >= 0;
        const base = hasExt ? filePath.substring(0, dotIdx) : filePath;
        if (numVar <= 1) return [base];
        const dir = lastSlash >= 0 ? base.substring(0, lastSlash + 1) : '';
        const name = lastSlash >= 0 ? base.substring(lastSlash + 1) : base;
        // For explicit file paths (with extension), only use numeric variation
        // expansion when the filename itself ends with digits.
        if (hasExt && !/\d+$/.test(name)) return [base];
        // Strip trailing digit only when path had an explicit extension.
        // Without extension the SLK already stores the base name (e.g. "Seaweed0"),
        // so variation indices are appended directly: Seaweed00, Seaweed01, …
        const variationBase = hasExt ? dir + name.replace(/\d+$/, '') : dir + name;
        const paths = [];
        for (let i = 0; i < numVar; i++) {
            paths.push(variationBase + i);
        }
        return paths;
    }

    // ── Shared model variants resolver (used by doodads/destructables) ──
    const _modelVariantsCache = {};
    const _modelVariantsPending = {};
    const _modelVariantsListeners = [];
    let _modelVariantsMessageBound = false;

    function _modelVariantsKey(filePath) {
        return String(filePath || '').toLowerCase();
    }

    function _emitModelVariantsResolved(filePath, variants, found) {
        for (let i = 0; i < _modelVariantsListeners.length; i++) {
            try {
                _modelVariantsListeners[i](filePath, variants, found);
            } catch (_) {}
        }
    }

    function _ensureModelVariantsMessageListener() {
        if (_modelVariantsMessageBound) return;
        _modelVariantsMessageBound = true;
        window.addEventListener('message', function (e) {
            const msg = e && e.data;
            if (!msg || msg.command !== 'modelVariantsResolved' || !msg.filePath) return;
            const key = _modelVariantsKey(msg.filePath);
            const variants = Array.isArray(msg.variants) ? msg.variants : [];
            const found = Array.isArray(msg.found) ? msg.found : [];
            _modelVariantsCache[key] = {variants: variants, found: found};
            delete _modelVariantsPending[key];
            _emitModelVariantsResolved(msg.filePath, variants, found);
        });
    }

    function resolveModelVariants(filePath, numVar, vscode) {
        if (!filePath) return {paths: [], resolving: false, found: []};
        _ensureModelVariantsMessageListener();
        const key = _modelVariantsKey(filePath);
        if (Object.prototype.hasOwnProperty.call(_modelVariantsCache, key)) {
            const data = _modelVariantsCache[key] || {};
            return {
                paths: Array.isArray(data.variants) ? data.variants : [],
                found: Array.isArray(data.found) ? data.found : [],
                resolving: false,
            };
        }
        if (!_modelVariantsPending[key] && vscode) {
            _modelVariantsPending[key] = true;
            vscode.postMessage({command: 'resolveModelVariants', filePath: filePath, numVar: numVar || 1});
        }
        return {paths: [], resolving: true, found: []};
    }

    function onModelVariantsResolved(callback) {
        if (typeof callback !== 'function') return function () {};
        _ensureModelVariantsMessageListener();
        _modelVariantsListeners.push(callback);
        return function () {
            const idx = _modelVariantsListeners.indexOf(callback);
            if (idx >= 0) _modelVariantsListeners.splice(idx, 1);
        };
    }

    function _hasDifferentDefault(currentValue, defaultValue) {
        return defaultValue !== undefined && String(currentValue ?? '') !== String(defaultValue ?? '');
    }

    function _modelLink(path, extraAttrs) {
        return '<a href="#" class="dd-model-link" data-path="' + esc(path) + '"' + (extraAttrs || '') + '>' + esc(path) + '</a>';
    }

    function _modelLinks(paths, extraAttrs) {
        return paths.map(function (path) {
            return _modelLink(path, extraAttrs);
        }).join('');
    }

    // Shared doodad/destructable model detail row.
    // includeTexAttrs=true adds data-tex-id/data-tex-file for destructables.
    function renderModelFileRow(opts) {
        const filePath = opts && opts.filePath ? String(opts.filePath) : '';
        if (!filePath) return '';
        const numVar = opts && opts.numVar ? opts.numVar : 1;
        const defaults = opts && opts.defaults ? opts.defaults : null;
        const vscode = opts && opts.vscode ? opts.vscode : null;
        const includeTexAttrs = !!(opts && opts.includeTexAttrs);
        const texId = opts && opts.texId ? opts.texId : 0;
        const texFile = opts && opts.texFile ? String(opts.texFile) : '';
        const extraAttrs = includeTexAttrs
            ? ((texId ? ' data-tex-id="' + texId + '"' : '')
                + (texFile ? ' data-tex-file="' + esc(texFile) + '"' : ''))
            : '';

        const currentInfo = resolveModelVariants(filePath, numVar || 1, vscode);
        const currentPaths = currentInfo.paths;
        const defaultFile = defaults && defaults.file !== undefined ? String(defaults.file) : undefined;
        const defaultNumVarRaw = defaults && defaults.numVar !== undefined ? defaults.numVar : undefined;
        const parsedDefaultNumVar = defaultNumVarRaw != null && String(defaultNumVarRaw).trim() !== ''
            ? Number(defaultNumVarRaw)
            : numVar || 1;
        const defaultNumVar = Number.isFinite(parsedDefaultNumVar) && parsedDefaultNumVar > 0 ? parsedDefaultNumVar : numVar || 1;
        const defaultInfo = defaultFile ? resolveModelVariants(defaultFile, defaultNumVar, vscode) : {paths: [], resolving: false};
        const defaultPaths = defaultInfo.paths;
        const showDefaultFile = _hasDifferentDefault(filePath, defaultFile);
        const showDefaultNames = defaultPaths.length > 0 && currentPaths.join('\n') !== defaultPaths.join('\n');

        let html = '<div class="dd-model-stack">'
            + '<div class="dd-model-label">As set</div>'
            + _modelLink(filePath, extraAttrs);

        if (currentPaths.length > 0) {
            html += '<div class="dd-model-label">' + (currentPaths.length > 1 ? 'Names' : 'Name') + '</div>'
                + _modelLinks(currentPaths, extraAttrs);
        } else if (currentInfo.resolving) {
            html += '<div class="dd-model-label">Names</div>'
                + '<div class="dd-default-value">Resolving variants...</div>';
        }

        if (showDefaultFile) {
            html += '<div class="dd-default-block">'
                + '<div class="dd-default-label">Default file</div>'
                + _modelLink(defaultFile, extraAttrs)
                + '</div>';
        }

        if (showDefaultNames) {
            html += '<div class="dd-default-block">'
                + '<div class="dd-default-label">' + (defaultPaths.length > 1 ? 'Default names' : 'Default name') + '</div>'
                + _modelLinks(defaultPaths, extraAttrs)
                + '</div>';
        } else if (showDefaultFile && defaultInfo.resolving) {
            html += '<div class="dd-default-block">'
                + '<div class="dd-default-label">Default names</div>'
                + '<div class="dd-default-value">Resolving variants...</div>'
                + '</div>';
        }

        html += '</div>';
        return '<tr class="dd-model-row"><td class="key">file</td><td class="dd-model-cell" colspan="2">' + html + '</td></tr>';
    }

    return {
        esc,
        indexToRgb,
        setWestrings,
        resolveWestring,
        gsValue,
        gsHtml,
        colorBadge,
        categoryBadge,
        tilesetBadges,
        buildModelPaths,
        resolveModelVariants,
        onModelVariantsResolved,
        renderModelFileRow,
    };
})();

