'use strict';

// ── Placed DOO objects — canvas list renderers & name resolution ─────

window._W3E_PLACED = (function () {
    let U = window._W3E_UTILS;
    let DOOD = window._W3E_DOODADS;
    let DEST = window._W3E_DESTRUCTABLES;
    let UNITS = window._W3E_UNITS;

    let _doodadDooItems = [];
    let _unitDooItems = [];
    let _destDooItems = [];

    let _unitDooCanvasList = null;
    let _doodadDooCanvasList = null;
    let _destDooCanvasList = null;

    function setDoodadDooItems(items) { _doodadDooItems = items || []; }
    function setUnitDooItems(items) { _unitDooItems = items || []; }
    function getDoodadDooItems() { return _doodadDooItems; }
    function getUnitDooItems() { return _unitDooItems; }

    function _fmtPlacedF(v) {
        return v != null ? Number(v).toFixed(1) : '—';
    }

    // ── Row renderers ─────────────────────────────────────────────
    function _renderPlacedDoodadRow(ctx, item, x, y, w, h, c) {
        let mid = y + h / 2;
        ctx.textBaseline = 'middle';
        ctx.font = '10px ' + c.mono;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        ctx.fillText(String(item.index + 1), x + 28, mid);
        ctx.textAlign = 'left';
        ctx.font = '11px ' + c.mono;
        ctx.fillStyle = item._error ? '#f44' : c.link;
        ctx.fillText(item.text || '', x + 34, mid);
        ctx.font = '10px ' + c.mono;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        let angleDeg = item.angle != null ? (item.angle * 180 / Math.PI).toFixed(0) + '\u00b0' : '';
        ctx.fillText(angleDeg, x + w, mid);
        let posText = _fmtPlacedF(item.position.x) + ', ' + _fmtPlacedF(item.position.y);
        ctx.fillText(posText, x + w - 42, mid);
        let posW = ctx.measureText(posText).width;
        ctx.textAlign = 'left';
        let nameX = x + 78;
        let nameEnd = x + w - 42 - posW - 12;
        let nameW = nameEnd - nameX;
        if (nameW > 10) {
            ctx.font = '12px ' + c.font;
            ctx.fillStyle = c.fg;
            _clTruncText(ctx, item._name || '', nameX, mid, nameW);
        }
        ctx.textBaseline = 'alphabetic';
    }

    function _renderPlacedUnitRow(ctx, item, x, y, w, h, c) {
        let mid = y + h / 2;
        ctx.textBaseline = 'middle';
        ctx.font = '10px ' + c.mono;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        ctx.fillText(String(item.index + 1), x + 28, mid);
        ctx.textAlign = 'left';
        ctx.font = '11px ' + c.mono;
        ctx.fillStyle = c.link;
        ctx.fillText(item.text || '', x + 34, mid);
        ctx.font = '10px ' + c.mono;
        ctx.fillStyle = c.desc;
        ctx.textAlign = 'right';
        if (item.player != null) {
            ctx.fillText('P' + item.player, x + w, mid);
        }
        let angleDeg = item.angle != null ? (item.angle * 180 / Math.PI).toFixed(0) + '\u00b0' : '';
        ctx.fillText(angleDeg, x + w - 28, mid);
        let posText = _fmtPlacedF(item.position.x) + ', ' + _fmtPlacedF(item.position.y);
        ctx.fillText(posText, x + w - 68, mid);
        let posW = ctx.measureText(posText).width;
        ctx.textAlign = 'left';
        let nameX = x + 78;
        let nameEnd = x + w - 68 - posW - 12;
        let nameW = nameEnd - nameX;
        if (nameW > 10) {
            ctx.font = '12px ' + c.font;
            ctx.fillStyle = c.fg;
            _clTruncText(ctx, item._name || '', nameX, mid, nameW);
        }
        ctx.textBaseline = 'alphabetic';
    }

    // ── Categorize and resolve names ──────────────────────────────
    function _categorizePlacedItems() {
        let doodadDataMap = DOOD.getDataMap();
        let destructableDataMap = DEST.getDataMap();
        let destItems = [];

        for (let i = 0; i < _doodadDooItems.length; i++) {
            let it = _doodadDooItems[i];
            let rawKey = String(it.raw);
            if (doodadDataMap[rawKey]) {
                it._name = U.gsValue(doodadDataMap[rawKey].name);
                it._error = false;
            } else if (destructableDataMap[rawKey]) {
                let dObj = destructableDataMap[rawKey];
                let rn = U.gsValue(dObj.name);
                let rs = U.gsValue(dObj.editorSuffix);
                it._name = rn + (rs ? ' ' + rs : '');
                it._error = false;
                destItems.push(it);
            } else {
                it._name = '';
                it._error = true;
                destItems.push(it);
            }
        }

        _destDooItems = destItems;

        let titleEl = document.getElementById('destDooTitle');
        if (titleEl) {
            titleEl.textContent = '\ud83c\udfda Placed Destructables (' + destItems.length + ')';
        }

        if (_doodadDooCanvasList) {
            _doodadDooCanvasList.setData(_doodadDooItems);
        }
        if (_destDooCanvasList) {
            _destDooCanvasList.setData(destItems);
        }
    }

    function updatePlacedNames() {
        let unitDataMap = UNITS.getDataMap();
        if (_doodadDooItems.length && DOOD.isLoaded() && DEST.isLoaded()) {
            _categorizePlacedItems();
        }
        for (let j = 0; j < _unitDooItems.length; j++) {
            let u = _unitDooItems[j];
            let rawKey = String(u.raw);
            let uObj = unitDataMap[rawKey];
            u._name = uObj ? (U.gsValue(uObj.name) || uObj.comment || '') : '';
        }
        if (_unitDooCanvasList) {
            _unitDooCanvasList.setData(_unitDooItems);
        }
    }

    // ── Canvas list lifecycle ─────────────────────────────────────
    function ensureUnitDooCanvasList() {
        if (_unitDooCanvasList) return;
        let el = document.getElementById('unitDooList');
        if (!el) return;
        _unitDooCanvasList = new CanvasList(el, {
            rowHeight: 26,
            renderRow: _renderPlacedUnitRow,
            onClick: function (item) {
                let rawKey = String(item.raw);
                if (UNITS.getDataMap()[rawKey]) {
                    UNITS.showDetail(rawKey);
                }
            }
        });
        if (_unitDooItems.length) _unitDooCanvasList.setData(_unitDooItems);
    }
    function disposeUnitDooCanvasList() {
        if (_unitDooCanvasList) { _unitDooCanvasList.dispose(); _unitDooCanvasList = null; }
    }

    function ensureDoodadDooCanvasList() {
        if (_doodadDooCanvasList) return;
        let el = document.getElementById('doodadDooList');
        if (!el) return;
        _doodadDooCanvasList = new CanvasList(el, {
            rowHeight: 26,
            renderRow: _renderPlacedDoodadRow,
            onClick: function (item) {
                let rawKey = String(item.raw);
                if (DOOD.getDataMap()[rawKey]) {
                    DOOD.showDetail(rawKey);
                } else if (DEST.getDataMap()[rawKey]) {
                    DEST.showDetail(rawKey);
                }
            }
        });
        if (_doodadDooItems.length) _doodadDooCanvasList.setData(_doodadDooItems);
    }
    function disposeDoodadDooCanvasList() {
        if (_doodadDooCanvasList) { _doodadDooCanvasList.dispose(); _doodadDooCanvasList = null; }
    }

    function ensureDestDooCanvasList() {
        if (_destDooCanvasList) return;
        let el = document.getElementById('destructableDooList');
        if (!el) return;
        _destDooCanvasList = new CanvasList(el, {
            rowHeight: 26,
            renderRow: _renderPlacedDoodadRow,
            onClick: function (item) {
                let rawKey = String(item.raw);
                if (DEST.getDataMap()[rawKey]) {
                    DEST.showDetail(rawKey);
                } else if (DOOD.getDataMap()[rawKey]) {
                    DOOD.showDetail(rawKey);
                }
            }
        });
        if (_destDooItems.length) _destDooCanvasList.setData(_destDooItems);
    }
    function disposeDestDooCanvasList() {
        if (_destDooCanvasList) { _destDooCanvasList.dispose(); _destDooCanvasList = null; }
    }

    // ── Highlight placed doodad ──────────────────────────────────
    function highlightPlacedDoodad(dooIndex) {
        let foundIdx = -1;
        for (let i = 0; i < _doodadDooItems.length; i++) {
            if (_doodadDooItems[i].index === dooIndex) { foundIdx = i; break; }
        }
        if (foundIdx < 0) return;
        let win = document.getElementById('doodadDooWindow');
        if (!win) return;
        win.show();
        ensureDoodadDooCanvasList();
        if (_doodadDooCanvasList) {
            _doodadDooCanvasList.scrollToIndex(foundIdx);
        }
    }

    return {
        setDoodadDooItems,
        setUnitDooItems,
        getDoodadDooItems,
        getUnitDooItems,
        updatePlacedNames,
        ensureUnitDooCanvasList,
        disposeUnitDooCanvasList,
        ensureDoodadDooCanvasList,
        disposeDoodadDooCanvasList,
        ensureDestDooCanvasList,
        disposeDestDooCanvasList,
        highlightPlacedDoodad,
    };
})();

