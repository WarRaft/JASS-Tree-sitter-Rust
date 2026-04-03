'use strict';

// ── Game Path body builder ──────────────────────────────────────────

window._W3E_GAME_PATH = (function () {
    var U = window._W3E_UTILS;

    const REQUIRED_MPQ = ['War3.mpq', 'War3x.mpq', 'War3xLocal.mpq', 'War3Patch.mpq'];

    function renderBody(status) {
        const gp = status.gamePath || '';
        const has = !!gp;
        let h = '<div class="gp-hint">Path to Warcraft III installation folder.</div>';
        h += has
            ? '<div class="gp-path">' + U.esc(gp) + '</div>'
            : '<div class="gp-no-path">Not selected</div>';
        if (has && status.mpqStatus) {
            h += '<div class="gp-mpq-list">';
            for (const f of REQUIRED_MPQ) {
                const ok = status.mpqStatus[f];
                h += '<div class="gp-mpq-row ' + (ok ? 'gp-ok' : 'gp-missing') + '">'
                    + '<span>' + (ok ? '\u2705' : '\u274c') + '</span> '
                    + '<span>' + U.esc(f) + '</span></div>';
            }
            h += '</div>';
        }
        h += '<div class="gp-actions">'
            + '<button class="gp-browse" id="gamePathBrowse">\ud83d\udcc2 Browse\u2026</button>';
        if (has) h += '<button class="gp-clear" id="gamePathClear">\u2715 Clear</button>';
        h += '</div>';
        return h;
    }

    return { renderBody };
})();

