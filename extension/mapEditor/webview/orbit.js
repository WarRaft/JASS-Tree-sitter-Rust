'use strict';

// ── Blender-style orbit controls (quaternion-based, no gimbal lock) ──
//
// Mouse (3-button):
//   MMB drag           → orbit (rotate)
//   Shift + MMB drag   → pan
//   Ctrl + MMB drag    → zoom
//   RMB drag           → pan
//   Scroll wheel       → zoom in / out
//
// Trackpad (two-finger gestures):
//   Two-finger scroll              → orbit
//   Shift + two-finger scroll      → pan
//   Pressed trackpad + scroll      → pan (one-hand)
//   Ctrl + two-finger scroll       → zoom (also pinch)
//
// Keyboard (numpad):
//   Numpad 1 / Ctrl+1  → front / back
//   Numpad 3 / Ctrl+3  → right / left
//   Numpad 7 / Ctrl+7  → top / bottom
//   Numpad 5            → perspective / orthographic
//   Numpad 2/4/6/8      → incremental rotation (15°)

window._W3E_ORBIT = (function () {

    let DEG15 = Math.PI / 12;
    let ROTATE_SPEED = 0.005;
    let PAN_SPEED = 1.0;
    let ANIM_DURATION = 200;

    function makeOrbitControls(cam, domEl, maxD, opts) {
        let skipGuards = opts && opts.skipGuards;
        let zUp = !!(opts && opts.zUp);

        let _maxD = maxD;
        let ZOOM_LINEAR = _maxD * 0.05;     // fixed dolly step per scroll notch
        let PAN_FIXED   = _maxD / 1000;     // fixed pan factor (world units per pixel)

        let target   = new THREE.Vector3();
        let panOff   = new THREE.Vector3();
        let dollyDelta = 0;               // forward/backward translation (dolly)
        let hDelta = 0, vDelta = 0;

        let orbiting = false, panning = false, zooming = false;
        let lmbDown = false;
        let px = 0, py = 0;

        // ── Quaternion orbit state ──────────────────────────────
        let _worldUp = zUp ? new THREE.Vector3(0, 0, 1) : new THREE.Vector3(0, 1, 0);
        let _orbit   = new THREE.Quaternion();
        let _radius  = maxD;
        let _needsInit = true;
        let _tmpV = new THREE.Vector3();
        let _tmpQ = new THREE.Quaternion();

        function _buildOrbitQuat(offsetDir) {
            let z = offsetDir.clone().normalize();
            let x = new THREE.Vector3().crossVectors(_worldUp, z);
            if (x.lengthSq() < 1e-6) {
                let fb = Math.abs(_worldUp.z) > 0.9
                    ? new THREE.Vector3(0, 1, 0)
                    : new THREE.Vector3(0, 0, 1);
                x.crossVectors(fb, z);
            }
            x.normalize();
            let y = new THREE.Vector3().crossVectors(z, x);
            let m = new THREE.Matrix4().makeBasis(x, y, z);
            return new THREE.Quaternion().setFromRotationMatrix(m);
        }

        function _initFromCamera() {
            let activeCam = getActiveCam();
            _radius = activeCam.position.distanceTo(target) || maxD;
            let dir = activeCam.position.clone().sub(target);
            _orbit.copy(_buildOrbitQuat(dir));
            _needsInit = false;
        }

        // ── Smooth animation ────────────────────────────────────
        let animating = false;
        let animStart = 0;
        let _animFrom = new THREE.Quaternion();
        let _animTo   = new THREE.Quaternion();

        // ── Perspective / Orthographic toggle ───────────────────
        let isPerspective = true;
        let orthoCam = null;
        function getActiveCam() { return isPerspective ? cam : orthoCam; }

        function ensureOrthoCam() {
            if (orthoCam) return;
            let aspect = cam.aspect || domEl.clientWidth / domEl.clientHeight || 1;
            let hh = _radius * 0.5, hw = hh * aspect;
            orthoCam = new THREE.OrthographicCamera(-hw, hw, hh, -hh, cam.near, cam.far);
            orthoCam.position.copy(cam.position);
            orthoCam.quaternion.copy(cam.quaternion);
        }

        function syncOrthoFrustum() {
            if (!orthoCam) return;
            let aspect = domEl.clientWidth / domEl.clientHeight || 1;
            let hh = _radius * 0.5, hw = hh * aspect;
            orthoCam.left = -hw; orthoCam.right = hw;
            orthoCam.top = hh;   orthoCam.bottom = -hh;
            orthoCam.near = cam.near; orthoCam.far = cam.far;
            orthoCam.updateProjectionMatrix();
        }

        // ── Pointer events ──────────────────────────────────────
        domEl.addEventListener('pointerdown', function (e) {
            if (!skipGuards && (e.target.closest('float-window') || e.target.closest('.menubar'))) return;
            if (e.button === 1) {
                if (e.shiftKey) panning = true;
                else if (e.ctrlKey || e.metaKey) zooming = true;
                else orbiting = true;
            } else if (e.button === 0) { lmbDown = true; }
            else if (e.button === 2) { panning = true; }
            px = e.clientX; py = e.clientY;
            if (orbiting || panning || zooming) domEl.setPointerCapture(e.pointerId);
        });

        domEl.addEventListener('pointermove', function (e) {
            let dx = e.clientX - px, dy = e.clientY - py;
            px = e.clientX; py = e.clientY;
            if (orbiting) { hDelta -= dx * ROTATE_SPEED; vDelta -= dy * ROTATE_SPEED; }
            if (zooming) { dollyDelta += dy / 50 * ZOOM_LINEAR; }
            if (panning) {
                let v = new THREE.Vector3(), activeCam = getActiveCam();
                v.setFromMatrixColumn(activeCam.matrix, 0); panOff.addScaledVector(v, -dx * PAN_FIXED * PAN_SPEED);
                v.setFromMatrixColumn(activeCam.matrix, 1); panOff.addScaledVector(v,  dy * PAN_FIXED * PAN_SPEED);
            }
        });

        domEl.addEventListener('pointerup', function (e) {
            orbiting = false; panning = false; zooming = false;
            if (e.button === 0) lmbDown = false;
            try { domEl.releasePointerCapture(e.pointerId); } catch (_) {}
        });

        // ── Wheel / scroll ──────────────────────────────────────
        domEl.addEventListener('wheel', function (e) {
            if (!skipGuards && e.target.closest('float-window')) return;
            e.preventDefault();
            if (e.deltaMode === 1) { dollyDelta += (e.deltaY > 0 ? 1 : -1) * ZOOM_LINEAR; return; }
            if (e.ctrlKey || e.metaKey) {
                dollyDelta += e.deltaY / 100 * ZOOM_LINEAR;
            } else if (e.shiftKey || lmbDown) {
                let activeCam = getActiveCam();
                let v = new THREE.Vector3();
                v.setFromMatrixColumn(activeCam.matrix, 0); panOff.addScaledVector(v, e.deltaX * PAN_FIXED * PAN_SPEED * 0.5);
                v.setFromMatrixColumn(activeCam.matrix, 1); panOff.addScaledVector(v, -e.deltaY * PAN_FIXED * PAN_SPEED * 0.5);
            } else {
                hDelta += e.deltaX * ROTATE_SPEED * 0.5;
                vDelta -= e.deltaY * ROTATE_SPEED * 0.5;
            }
        }, {passive: false});

        domEl.addEventListener('contextmenu', function (e) { e.preventDefault(); });

        // ── Keyboard ────────────────────────────────────────────
        domEl.setAttribute('tabindex', '0');
        domEl.style.outline = 'none';
        domEl.addEventListener('keydown', function (e) {
            if (!skipGuards && (e.target.closest('float-window') || e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA')) return;
            let handled = true;
            switch (e.code) {
                case 'Numpad1': animateTo(Math.PI / 2, e.ctrlKey || e.metaKey ? Math.PI : 0); break;
                case 'Numpad3': animateTo(Math.PI / 2, e.ctrlKey || e.metaKey ? -Math.PI / 2 : Math.PI / 2); break;
                case 'Numpad7': animateTo(e.ctrlKey || e.metaKey ? Math.PI - 0.001 : 0.001, 0); break;
                case 'Numpad5': isPerspective = !isPerspective; if (!isPerspective) { ensureOrthoCam(); syncOrthoFrustum(); } break;
                case 'Numpad4': hDelta += DEG15; break;
                case 'Numpad6': hDelta -= DEG15; break;
                case 'Numpad8': vDelta -= DEG15; break;
                case 'Numpad2': vDelta += DEG15; break;
                default: handled = false;
            }
            if (handled) e.preventDefault();
        });

        // ── Animation to preset view ────────────────────────────
        function animateTo(phi, theta) {
            let tmpSph = new THREE.Spherical(1, phi, theta);
            let dir = new THREE.Vector3().setFromSpherical(tmpSph);
            if (zUp) { let t = dir.y; dir.y = dir.z; dir.z = t; }
            _animFrom.copy(_orbit);
            _animTo.copy(_buildOrbitQuat(dir));
            animStart = performance.now();
            animating = true;
        }

        function easeInOut(t) { return t < 0.5 ? 2*t*t : -1 + (4 - 2*t) * t; }

        // ── Axis gizmo ──────────────────────────────────────────
        let gizmoRenderer = null, gizmoScene = null, gizmoCamera = null;
        (function initGizmo() {
            try {
                let SIZE = 120, ALEN = 1.0;
                let gCanvas = document.createElement('canvas');
                gCanvas.style.cssText = 'position:absolute;top:8px;right:8px;width:'+SIZE+'px;height:'+SIZE+'px;pointer-events:none;z-index:100;';
                let gParent = domEl.parentElement || document.body;
                let pp = window.getComputedStyle(gParent).position;
                if (!pp || pp === 'static') gParent.style.position = 'relative';
                gParent.appendChild(gCanvas);
                gizmoRenderer = new THREE.WebGLRenderer({canvas: gCanvas, alpha: true, antialias: true});
                gizmoRenderer.setPixelRatio(window.devicePixelRatio);
                gizmoRenderer.setSize(SIZE, SIZE);
                gizmoRenderer.setClearColor(0x000000, 0);
                gizmoScene = new THREE.Scene();
                gizmoCamera = new THREE.OrthographicCamera(-1.8, 1.8, 1.8, -1.8, 0.1, 100);

                function axisLine(to, c) { gizmoScene.add(new THREE.Line(new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), to]), new THREE.LineBasicMaterial({color: c}))); }
                function tip(p, c, r) { let m = new THREE.Mesh(new THREE.SphereGeometry(r,12,12), new THREE.MeshBasicMaterial({color:c})); m.position.copy(p); gizmoScene.add(m); }
                function label(text, hex, pos) {
                    let c = document.createElement('canvas'); c.width=64; c.height=64;
                    let ctx = c.getContext('2d'); ctx.font='bold 48px Arial,sans-serif'; ctx.textAlign='center'; ctx.textBaseline='middle';
                    ctx.fillStyle='#222'; ctx.fillText(text,33,33); ctx.fillStyle=hex; ctx.fillText(text,32,32);
                    let s = new THREE.Sprite(new THREE.SpriteMaterial({map:new THREE.CanvasTexture(c),depthTest:false,transparent:true}));
                    s.position.copy(pos); s.scale.set(0.45,0.45,1); gizmoScene.add(s);
                }
                axisLine(new THREE.Vector3(ALEN,0,0),0xE34040); tip(new THREE.Vector3(ALEN,0,0),0xE34040,0.12); label('X','#E34040',new THREE.Vector3(ALEN+0.3,0,0));
                axisLine(new THREE.Vector3(0,ALEN,0),0x6CCF44); tip(new THREE.Vector3(0,ALEN,0),0x6CCF44,0.12); label('Y','#6CCF44',new THREE.Vector3(0,ALEN+0.3,0));
                axisLine(new THREE.Vector3(0,0,ALEN),0x4B8BE5); tip(new THREE.Vector3(0,0,ALEN),0x4B8BE5,0.12); label('Z','#4B8BE5',new THREE.Vector3(0,0,ALEN+0.3));
                tip(new THREE.Vector3(-ALEN*0.5,0,0),0x802020,0.07); tip(new THREE.Vector3(0,-ALEN*0.5,0),0x3A6A22,0.07); tip(new THREE.Vector3(0,0,-ALEN*0.5),0x274A78,0.07);
                tip(new THREE.Vector3(),0x888888,0.08);
            } catch(_) { gizmoRenderer = null; }
        })();

        // ── Update ──────────────────────────────────────────────
        let _initPos = cam.position.clone();
        let _initTarget = new THREE.Vector3(); // will be set via ctrl.target before first update

        let ctrl = {
            target: target,
            get maxDist() { return _maxD; },
            set maxDist(v) { _maxD = v; ZOOM_LINEAR = v * 0.05; PAN_FIXED = v / 1000; },
            get camera() { return getActiveCam(); },

            reset: function () {
                target.copy(_initTarget);
                cam.position.copy(_initPos);
                cam.lookAt(target);
                _radius = _initPos.distanceTo(target) || maxD;
                let dir = cam.position.clone().sub(target);
                _orbit.copy(_buildOrbitQuat(dir));
                if (orthoCam) {
                    orthoCam.position.copy(cam.position);
                    orthoCam.quaternion.copy(cam.quaternion);
                    syncOrthoFrustum();
                }
            },

            /** Call after setting target to remember the initial state for reset(). */
            saveInitState: function () {
                _initTarget.copy(target);
                _initPos.copy(cam.position);
            },

            update: function () {
                if (_needsInit) _initFromCamera();
                let activeCam = getActiveCam();

                if (animating) {
                    let t = Math.min(1, (performance.now() - animStart) / ANIM_DURATION);
                    t = easeInOut(t);
                    _orbit.slerpQuaternions(_animFrom, _animTo, t);
                    if (t >= 1) animating = false;
                } else {
                    if (hDelta !== 0) {
                        _tmpQ.setFromAxisAngle(_worldUp, hDelta);
                        _orbit.premultiply(_tmpQ);
                    }
                    if (vDelta !== 0) {
                        _tmpV.set(1, 0, 0).applyQuaternion(_orbit);
                        _tmpQ.setFromAxisAngle(_tmpV, -vDelta);
                        _orbit.premultiply(_tmpQ);
                    }
                }

                _orbit.normalize();

                // Dolly: translate target along the view direction (back axis)
                // so the entire orbit sphere moves forward/backward.
                // This keeps the orbit center meaningful at any distance
                // and allows flying through any point — no radius limits.
                if (dollyDelta !== 0) {
                    _tmpV.set(0, 0, 1).applyQuaternion(_orbit);
                    target.addScaledVector(_tmpV, dollyDelta);
                }

                target.add(panOff);

                _tmpV.set(0, 0, _radius).applyQuaternion(_orbit);
                activeCam.position.copy(target).add(_tmpV);
                activeCam.quaternion.copy(_orbit);
                activeCam.updateMatrixWorld(true);

                if (!isPerspective) syncOrthoFrustum();
                if (isPerspective && orthoCam) { orthoCam.position.copy(cam.position); orthoCam.quaternion.copy(cam.quaternion); }
                else if (!isPerspective) { cam.position.copy(orthoCam.position); cam.quaternion.copy(orthoCam.quaternion); }

                hDelta = 0; vDelta = 0;
                panOff.set(0, 0, 0);
                dollyDelta = 0;

                if (gizmoRenderer) {
                    gizmoCamera.quaternion.copy(_orbit);
                    gizmoCamera.position.set(0, 0, 5).applyQuaternion(_orbit);
                    gizmoRenderer.render(gizmoScene, gizmoCamera);
                }
            }
        };
        return ctrl;
    }

    return { makeOrbitControls: makeOrbitControls };
})();
