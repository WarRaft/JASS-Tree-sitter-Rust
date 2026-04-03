'use strict';

// ── Shared orbit controls (used by terrain & model viewer) ──────────

window._W3E_ORBIT = (function () {

    function makeOrbitControls(cam, domEl, maxD, opts) {
        const skipGuards = opts && opts.skipGuards;
        const target = new THREE.Vector3();
        const sph = new THREE.Spherical();
        const sphDelta = new THREE.Spherical();
        const panOff = new THREE.Vector3();
        let zoomFactor = 1;
        const ROTATE_SPEED = 0.005, PAN_SPEED = 1.0;
        let rotating = false, panning = false, px = 0, py = 0;

        domEl.addEventListener('pointerdown', function (e) {
            if (!skipGuards && (e.target.closest('float-window') || e.target.closest('.menubar'))) return;
            if (e.button === 0) rotating = true;
            else if (e.button === 1 || e.button === 2) panning = true;
            px = e.clientX; py = e.clientY;
            domEl.setPointerCapture(e.pointerId);
        });
        domEl.addEventListener('pointermove', function (e) {
            var dx = e.clientX - px, dy = e.clientY - py;
            px = e.clientX; py = e.clientY;
            if (rotating) {
                sphDelta.theta -= dx * ROTATE_SPEED;
                sphDelta.phi -= dy * ROTATE_SPEED;
            }
            if (panning) {
                var v = new THREE.Vector3();
                var factor = cam.position.distanceTo(target) * Math.tan(cam.fov / 2 * Math.PI / 180) * 2 / domEl.clientHeight;
                v.setFromMatrixColumn(cam.matrix, 0);
                panOff.addScaledVector(v, -dx * factor * PAN_SPEED);
                v.setFromMatrixColumn(cam.matrix, 1);
                panOff.addScaledVector(v, dy * factor * PAN_SPEED);
            }
        });
        domEl.addEventListener('pointerup', function (e) {
            rotating = false; panning = false;
            try { domEl.releasePointerCapture(e.pointerId); } catch (_) {}
        });
        domEl.addEventListener('wheel', function (e) {
            if (!skipGuards && e.target.closest('float-window')) return;
            e.preventDefault();
            zoomFactor *= e.deltaY > 0 ? 1.1 : 0.9;
        }, {passive: false});
        domEl.addEventListener('contextmenu', function (e) { e.preventDefault(); });

        var ctrl = {
            target: target,
            maxDist: maxD,
            update: function () {
                var off = cam.position.clone().sub(target);
                sph.setFromVector3(off);
                sph.theta += sphDelta.theta;
                sph.phi += sphDelta.phi;
                sph.phi = Math.max(0.01, Math.min(Math.PI - 0.01, sph.phi));
                sph.radius *= zoomFactor;
                sph.radius = Math.max(1, Math.min(ctrl.maxDist * 5, sph.radius));
                target.add(panOff);
                off.setFromSpherical(sph);
                cam.position.copy(target).add(off);
                cam.lookAt(target);
                sphDelta.set(0, 0, 0);
                panOff.set(0, 0, 0);
                zoomFactor = 1;
            }
        };
        return ctrl;
    }

    return { makeOrbitControls };
})();

