'use strict';

// ── Path Texture viewer ─────────────────────────────────────────────

window._W3E_PATH_TEX = (function () {
    let U = window._W3E_UTILS;

    function showPathTex(texPath) {
        let win = document.getElementById('pathTexWindow');
        let body = document.getElementById('pathTexBody');
        if (!win || !body) return;

        win.setAttribute('title-text', '\ud83d\udea7 ' + texPath.replace(/\\/g, '/').split('/').pop());
        win.show();
        body.innerHTML = '<div class="ptex-loading">\u231b Loading\u2026</div>';

        let data = window.__W3E_DATA__;
        if (!data || !data.binaryServer) {
            body.innerHTML = '<div class="ptex-error">\u26a0 Binary server not available</div>';
            return;
        }

        let bs = data.binaryServer;
        let params = new URLSearchParams({token: bs.token, path: texPath});
        if (data.isArchive && data.archivePath) params.set('archive', data.archivePath);

        fetch('http://127.0.0.1:' + bs.port + '/mapEditor/pathTex?' + params)
            .then(function (resp) {
                if (!resp.ok) throw new Error('HTTP ' + resp.status);
                return resp.json();
            })
            .then(function (result) {
                _renderPathTexGrid(body, result, texPath);
            })
            .catch(function (err) {
                body.innerHTML = '<div class="ptex-error">\u26a0 ' + U.esc(String(err)) + '</div>';
            });
    }

    function _renderPathTexGrid(container, result, texPath) {
        let w = result.width;
        let h = result.height;
        let px = result.pixels;

        let html = '<div class="ptex-legend">'
            + '<div class="ptex-legend-row">'
            + '<div class="ptex-legend-cell">'
            + '<span style="background:#e53935"></span>'
            + '<span style="background:#43a047"></span>'
            + '<span style="background:#1e88e5"></span>'
            + '<span style="background:#666"></span>'
            + '</div>'
            + '<span>\u2190</span>'
            + '</div>'
            + '<div class="ptex-legend-row">'
            + '<span style="color:#e53935">\u25cf</span> 1 Walk'
            + '<span>\u2003</span>'
            + '<span style="color:#43a047">\u25cf</span> 2 Fly'
            + '</div>'
            + '<div class="ptex-legend-row">'
            + '<span style="color:#1e88e5">\u25cf</span> 3 Build'
            + '<span>\u2003</span>'
            + '<span style="background:#666;display:inline-block;width:10px;height:10px;border-radius:2px;vertical-align:middle;border:1px solid rgba(255,255,255,0.2);"></span> 4 Color'
            + '</div>'
            + '</div>';

        html += '<div class="ptex-source">' + U.esc(texPath) + ' \u2014 ' + w + '\u00d7' + h + ' \u2014 source: ' + U.esc(result.source) + '</div>';

        html += '<div class="ptex-grid" style="grid-template-columns:repeat(' + w + ', 24px);">';

        for (let y = 0; y < h; y++) {
            for (let x = 0; x < w; x++) {
                let idx = (y * w + x) * 3;
                let r = px[idx];
                let g = px[idx + 1];
                let b = px[idx + 2];

                let canWalk = (r === 0);
                let canFly = (g === 0);
                let canBuild = (b === 0);

                let walkColor = canWalk ? '#e53935' : 'rgba(229,57,53,0.12)';
                let flyColor = canFly ? '#43a047' : 'rgba(67,160,71,0.12)';
                let buildColor = canBuild ? '#1e88e5' : 'rgba(30,136,229,0.12)';
                let rgbColor = 'rgb(' + r + ',' + g + ',' + b + ')';

                let title = 'x=' + x + ' y=' + y
                    + '  R=' + r + ' G=' + g + ' B=' + b
                    + '\nWalk: ' + (canWalk ? 'YES' : 'no')
                    + '  Fly: ' + (canFly ? 'YES' : 'no')
                    + '  Build: ' + (canBuild ? 'YES' : 'no');

                html += '<div class="ptex-cell" title="' + U.esc(title) + '">'
                    + '<span style="background:' + walkColor + '"></span>'
                    + '<span style="background:' + flyColor + '"></span>'
                    + '<span style="background:' + buildColor + '"></span>'
                    + '<span style="background:' + rgbColor + '"></span>'
                    + '</div>';
            }
        }

        html += '</div>';
        container.innerHTML = html;
    }

    return { showPathTex };
})();

