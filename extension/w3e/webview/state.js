'use strict';

// ── Webview state persistence helpers ────────────────────────────────
// Shared by doodads, destructables, units modules for persisting
// sort/filter/collapse state in VS Code webview state.

window._W3E_STATE = (function () {
    var _vscode = null;

    function setVscode(v) { _vscode = v; }
    function getVscode() { return _vscode; }

    function getWvState() {
        if (!_vscode) return {};
        try { return _vscode.getState() || {}; } catch (_) { return {}; }
    }

    function patchWvState(patch) {
        if (!_vscode) return;
        try {
            const s = getWvState();
            Object.assign(s, patch);
            _vscode.setState(s);
        } catch (_) { /* ignore */ }
    }

    return { setVscode, getVscode, getWvState, patchWvState };
})();

