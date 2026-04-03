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
    var _westringsMap = {};

    function setWestrings(map) {
        _westringsMap = (map && typeof map === 'object') ? map : {};
    }

    function resolveWestring(val) {
        if (!val || typeof val !== 'string') return val || '';
        var current = val;
        for (var i = 0; i < 3; i++) {
            if (!current.startsWith('WESTRING_')) break;
            var resolved = _westringsMap[current];
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
            var v = esc(gs.value);
            if (gs.original && gs.original !== gs.value) {
                return '<a href="#" class="gs-resolved" data-gs-original="' + esc(gs.original) + '" data-gs-source="' + esc(gs.source || '') + '">' + v + '</a>';
            }
            return v;
        }
        return esc(String(gs));
    }

    function _showGameStringInfo(value, original, source) {
        var win = document.getElementById('gameStringInfoWindow');
        if (!win) return;
        var body = win.querySelector('#gsInfoBody');
        if (!body) return;
        body.innerHTML =
            '<table class="info">' +
            '<tr><td class="key">value</td><td>' + esc(value) + '</td></tr>' +
            '<tr><td class="key">original</td><td><span class="code">' + esc(original) + '</span></td></tr>' +
            '<tr><td class="key">source</td><td>' + esc(source) + '</td></tr>' +
            '</table>';
        win.setAttribute('title-text', '\ud83d\udd17 ' + value);
        win.show();
    }

    document.addEventListener('click', function (e) {
        var link = e.target.closest('.gs-resolved');
        if (!link) return;
        e.preventDefault();
        var original = link.getAttribute('data-gs-original') || '';
        var source = link.getAttribute('data-gs-source') || '';
        var value = link.textContent || '';
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
        const ext = hasExt ? filePath.substring(dotIdx) : '.mdx';
        if (numVar <= 1) return [base + ext];
        const paths = [];
        for (let i = 0; i < numVar; i++) {
            paths.push(base + i + ext);
        }
        return paths;
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
    };
})();

