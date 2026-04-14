'use strict';

// ── CanvasList — Virtual-scroll canvas-based list ───────────────────
// Replaces heavy DOM lists (hundreds of shadow-DOM custom elements)
// with a single <canvas>. Only visible rows are drawn.
// Handles wheel-scroll, hover, click.

function _clRoundRect(ctx, x, y, w, h, r) {
    ctx.beginPath();
    ctx.moveTo(x + r, y);
    ctx.lineTo(x + w - r, y);
    ctx.quadraticCurveTo(x + w, y, x + w, y + r);
    ctx.lineTo(x + w, y + h - r);
    ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
    ctx.lineTo(x + r, y + h);
    ctx.quadraticCurveTo(x, y + h, x, y + h - r);
    ctx.lineTo(x, y + r);
    ctx.quadraticCurveTo(x, y, x + r, y);
    ctx.closePath();
}

function _clTruncText(ctx, text, x, y, maxW) {
    if (!text) return;
    if (ctx.measureText(text).width <= maxW) { ctx.fillText(text, x, y); return; }
    let ew = ctx.measureText('\u2026').width;
    let t = text;
    while (t.length > 0 && ctx.measureText(t).width + ew > maxW) t = t.slice(0, -1);
    ctx.fillText(t + '\u2026', x, y);
}

function _clDrawBadge(ctx, ch, x, rowY, rowH, c, isAll) {
    let bw = 16, bh = 16, by = rowY + (rowH - bh) / 2;
    ctx.fillStyle = isAll ? c.badgeAllBg : c.badgeBg;
    _clRoundRect(ctx, x, by, bw, bh, 3);
    ctx.fill();
    ctx.font = '600 10px ' + c.mono;
    ctx.fillStyle = isAll ? c.badgeAllFg : c.desc;
    ctx.textAlign = 'center';
    ctx.fillText(ch, x + bw / 2, rowY + rowH / 2);
    ctx.textAlign = 'left';
}

class CanvasList {
    constructor(container, options) {
        this._container = container;
        this._rh = options.rowHeight || 26;
        this._renderRow = options.renderRow;
        this._onClick = options.onClick || null;
        this._items = [];
        this._scrollY = 0;
        this._hover = -1;
        this._w = 0;
        this._h = 0;
        this._disposed = false;
        this._raf = 0;
        this._scrollDragging = false;
        this._scrollDragStartY = 0;
        this._scrollDragStartScrollY = 0;
        this._highlight = -1;
        this._highlightTimeout = 0;

        let cs = getComputedStyle(document.documentElement);
        let cv = function (n, fb) { return cs.getPropertyValue(n).trim() || fb; };
        this.C = {
            bg: cv('--vscode-editor-background', '#1e1e1e'),
            fg: cv('--vscode-editor-foreground', '#ccc'),
            hover: cv('--vscode-list-hoverBackground', 'rgba(255,255,255,0.06)'),
            border: cv('--vscode-editorWidget-border', '#333'),
            link: cv('--vscode-textLink-foreground', '#3794ff'),
            desc: cv('--vscode-descriptionForeground', '#999'),
            font: cv('--vscode-font-family', 'sans-serif'),
            mono: cv('--vscode-editor-font-family', 'monospace'),
            badgeBg: 'rgba(255,255,255,0.08)',
            badgeAllBg: 'rgba(78,154,241,0.25)',
            badgeAllFg: cv('--vscode-textLink-foreground', '#3794ff'),
        };

        this._canvas = document.createElement('canvas');
        this._canvas.style.cssText = 'display:block;width:100%;height:100%;cursor:default;';
        this._ctx = this._canvas.getContext('2d');

        container.style.overflow = 'hidden';
        container.innerHTML = '';
        container.appendChild(this._canvas);

        let self = this;
        this._handlers = {
            wheel: function (e) { self._onWheel(e); },
            move: function (e) { self._onMove(e); },
            leave: function () { self._onLeave(); },
            click: function (e) { self._onClickEvt(e); },
            down: function (e) { self._onPointerDown(e); },
            pmove: function (e) { self._onPointerMove(e); },
            up: function (e) { self._onPointerUp(e); },
        };
        this._canvas.addEventListener('wheel', this._handlers.wheel, {passive: false});
        this._canvas.addEventListener('mousemove', this._handlers.move);
        this._canvas.addEventListener('mouseleave', this._handlers.leave);
        this._canvas.addEventListener('click', this._handlers.click);
        this._canvas.addEventListener('pointerdown', this._handlers.down);
        this._canvas.addEventListener('pointermove', this._handlers.pmove);
        this._canvas.addEventListener('pointerup', this._handlers.up);

        this._ro = new ResizeObserver(function () { self._resize(); });
        this._ro.observe(container);
        this._resize();
    }

    setData(items) {
        this._items = items || [];
        this._clamp();
        this._schedule();
    }

    _resize() {
        let dpr = window.devicePixelRatio || 1;
        let w = this._container.clientWidth;
        let h = this._container.clientHeight;
        if (w <= 0 || h <= 0) return;
        this._canvas.width = w * dpr;
        this._canvas.height = h * dpr;
        this._ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        this._w = w;
        this._h = h;
        this._clamp();
        this._draw();
    }

    _clamp() {
        let max = Math.max(0, this._items.length * this._rh - this._h);
        if (this._scrollY > max) this._scrollY = max;
        if (this._scrollY < 0) this._scrollY = 0;
    }

    _schedule() {
        if (this._raf) return;
        let self = this;
        this._raf = requestAnimationFrame(function () { self._raf = 0; self._draw(); });
    }

    _scrollbarMetrics() {
        let total = this._items.length * this._rh;
        if (total <= this._h) return null;
        let thumbH = Math.max(20, this._h * this._h / total);
        let thumbY = (this._scrollY / total) * this._h;
        return {thumbH: thumbH, thumbY: thumbY, trackX: this._w - 10, trackW: 8};
    }

    _draw() {
        if (this._disposed) return;
        let ctx = this._ctx, w = this._w, h = this._h;
        if (!w || !h) return;
        ctx.clearRect(0, 0, w, h);

        let rh = this._rh;
        let first = Math.floor(this._scrollY / rh);
        let last = Math.min(this._items.length - 1, Math.ceil((this._scrollY + h) / rh));

        for (let i = first; i <= last; i++) {
            let y = i * rh - this._scrollY;
            if (i === this._highlight) {
                ctx.fillStyle = 'rgba(55, 148, 255, 0.2)';
                ctx.fillRect(0, y, w, rh);
            } else if (i === this._hover) {
                ctx.fillStyle = this.C.hover;
                ctx.fillRect(0, y, w, rh);
            }
            ctx.fillStyle = this.C.border;
            ctx.fillRect(0, y + rh - 1, w, 1);
            this._renderRow(ctx, this._items[i], 6, y, w - 20, rh, this.C);
        }

        let sb = this._scrollbarMetrics();
        if (sb) {
            ctx.fillStyle = 'rgba(255,255,255,0.15)';
            _clRoundRect(ctx, sb.trackX, sb.thumbY, sb.trackW, sb.thumbH, 3);
            ctx.fill();
        }
    }

    _idx(clientY) {
        let rect = this._canvas.getBoundingClientRect();
        let y = clientY - rect.top;
        let i = Math.floor((y + this._scrollY) / this._rh);
        return i >= 0 && i < this._items.length ? i : -1;
    }

    _onWheel(e) {
        e.preventDefault();
        this._scrollY += e.deltaY;
        this._clamp();
        this._hover = this._idx(e.clientY);
        this._schedule();
    }

    _onMove(e) {
        let i = this._idx(e.clientY);
        if (i !== this._hover) { this._hover = i; this._schedule(); }
    }

    _onLeave() {
        if (this._hover !== -1) { this._hover = -1; this._schedule(); }
    }

    _onClickEvt(e) {
        if (this._scrollDragging) return;
        let i = this._idx(e.clientY);
        if (i >= 0 && this._onClick) this._onClick(this._items[i], i, e);
    }

    _onPointerDown(e) {
        let sb = this._scrollbarMetrics();
        if (!sb) return;
        let rect = this._canvas.getBoundingClientRect();
        let mx = e.clientX - rect.left;
        let my = e.clientY - rect.top;
        if (mx >= sb.trackX && mx <= sb.trackX + sb.trackW && my >= sb.thumbY && my <= sb.thumbY + sb.thumbH) {
            this._scrollDragging = true;
            this._scrollDragStartY = my;
            this._scrollDragStartScrollY = this._scrollY;
            this._canvas.setPointerCapture(e.pointerId);
            e.preventDefault();
        }
    }

    _onPointerMove(e) {
        if (!this._scrollDragging) return;
        let rect = this._canvas.getBoundingClientRect();
        let my = e.clientY - rect.top;
        let dy = my - this._scrollDragStartY;
        let total = this._items.length * this._rh;
        this._scrollY = this._scrollDragStartScrollY + dy * total / this._h;
        this._clamp();
        this._schedule();
    }

    _onPointerUp(e) {
        if (this._scrollDragging) {
            this._scrollDragging = false;
            try { this._canvas.releasePointerCapture(e.pointerId); } catch (_) {}
        }
    }

    scrollToIndex(idx) {
        if (idx < 0 || idx >= this._items.length) return;
        let y = idx * this._rh;
        if (y < this._scrollY || y + this._rh > this._scrollY + this._h) {
            this._scrollY = Math.max(0, y - this._h / 2 + this._rh / 2);
            this._clamp();
        }
        this._highlight = idx;
        this._schedule();
        let self = this;
        clearTimeout(this._highlightTimeout);
        this._highlightTimeout = setTimeout(function () {
            self._highlight = -1;
            self._schedule();
        }, 2000);
    }

    dispose() {
        this._disposed = true;
        clearTimeout(this._highlightTimeout);
        if (this._raf) { cancelAnimationFrame(this._raf); this._raf = 0; }
        this._ro.disconnect();
        this._canvas.removeEventListener('wheel', this._handlers.wheel);
        this._canvas.removeEventListener('mousemove', this._handlers.move);
        this._canvas.removeEventListener('mouseleave', this._handlers.leave);
        this._canvas.removeEventListener('click', this._handlers.click);
        this._canvas.removeEventListener('pointerdown', this._handlers.down);
        this._canvas.removeEventListener('pointermove', this._handlers.pmove);
        this._canvas.removeEventListener('pointerup', this._handlers.up);
        if (this._canvas.parentNode) this._canvas.parentNode.removeChild(this._canvas);
        this._items = [];
    }
}

