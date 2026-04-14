'use strict';

// ── FPS-style camera controls for terrain viewing ───────────────────
//
// Mouse (3-button):
//   MMB drag           → look around (yaw / pitch)
//   Shift + MMB drag   → pan (lateral)
//   Ctrl + MMB drag    → zoom (dolly)
//   RMB drag           → pan
//   Scroll wheel       → dolly forward / backward
//
// Trackpad (two-finger gestures):
//   Two-finger scroll              → look around (yaw / pitch)
//   Shift + two-finger scroll      → pan
//   Pressed trackpad + scroll      → pan (one-hand)
//   Ctrl + two-finger scroll       → zoom (dolly)
//
// Keyboard:
//   W / ArrowUp    → move forward
//   S / ArrowDown  → move backward
//   A / ArrowLeft  → strafe left
//   D / ArrowRight → strafe right
//   Q / Space      → move up
//   E / ShiftLeft  → move down
//   Shift (hold)   → faster movement
//
// Numpad (same as orbit):
//   Numpad 1 / Ctrl+1  → front / back
//   Numpad 3 / Ctrl+3  → right / left
//   Numpad 7 / Ctrl+7  → top / bottom
//   Numpad 5            → perspective / orthographic
//   Numpad 2/4/6/8      → incremental rotation (15°)

window._W3E_FPS = (function () {

    let DEG15 = Math.PI / 12;
    let ROTATE_SPEED = 0.005;
    let PAN_SPEED = 1.0;
    let ANIM_DURATION = 200;

    function makeFpsControls(cam, domEl, maxD, opts) {
        let skipGuards = opts && opts.skipGuards;
        let zUp = !!(opts && opts.zUp);

        let _maxD = maxD;
        let ZOOM_LINEAR = _maxD * 0.05;
        let PAN_FIXED   = _maxD / 1000;
        let MOVE_SPEED  = _maxD * 0.5;       // world-units per second for WASD

        let panOff   = new THREE.Vector3();
        let dollyDelta = 0;
        let hDelta = 0, vDelta = 0;

        let orbiting = false, panning = false, zooming = false;
        let lmbDown = false;
        let px = 0, py = 0;

        // ── Euler-based FPS orientation ─────────────────────────────
        let _worldUp = zUp ? new THREE.Vector3(0, 0, 1) : new THREE.Vector3(0, 1, 0);
        let _yaw   = 0;   // horizontal (around world-up)
        let _pitch = 0;   // vertical (clamped ±89°)
        let _needsInit = true;
        let _tmpV = new THREE.Vector3();

        // ── Keys held ───────────────────────────────────────────────
        let _keysDown = {};

        function _buildQuatFromYawPitch(yaw, pitch) {
            // Quaternion = yaw around world-up, then pitch around local-right
            let qYaw = new THREE.Quaternion();
            let qPitch = new THREE.Quaternion();
            qYaw.setFromAxisAngle(_worldUp, yaw);
            // right axis after yaw
            let right = new THREE.Vector3(1, 0, 0).applyQuaternion(qYaw);
            qPitch.setFromAxisAngle(right, -pitch);
            return qPitch.multiply(qYaw);
        }

        function _initFromCamera() {
            // Extract yaw/pitch from current camera orientation
            let fwd = new THREE.Vector3(0, 0, -1).applyQuaternion(cam.quaternion);
            if (zUp) {
                _yaw = Math.atan2(fwd.y, fwd.x) - Math.PI / 2;
                let hLen = Math.sqrt(fwd.x * fwd.x + fwd.y * fwd.y);
                _pitch = Math.atan2(fwd.z, hLen);
            } else {
                _yaw = Math.atan2(fwd.x, fwd.z);
                let hLen = Math.sqrt(fwd.x * fwd.x + fwd.z * fwd.z);
                _pitch = Math.atan2(fwd.y, hLen);
            }
            _needsInit = false;
        }

        // ── Smooth animation ────────────────────────────────────────
        let animating = false;
        let animStart = 0;
        let _animFromQ = new THREE.Quaternion();
        let _animToQ   = new THREE.Quaternion();
        let _animTargetYaw = 0;
        let _animTargetPitch = 0;

        // ── Perspective / Orthographic toggle ───────────────────────
        let isPerspective = true;
        let orthoCam = null;
        function getActiveCam() { return isPerspective ? cam : orthoCam; }

        function ensureOrthoCam() {
            if (orthoCam) return;
            let aspect = cam.aspect || domEl.clientWidth / domEl.clientHeight || 1;
            let hh = 500, hw = hh * aspect;
            orthoCam = new THREE.OrthographicCamera(-hw, hw, hh, -hh, cam.near, cam.far);
            orthoCam.position.copy(cam.position);
            orthoCam.quaternion.copy(cam.quaternion);
        }

        function syncOrthoFrustum() {
            if (!orthoCam) return;
            let aspect = domEl.clientWidth / domEl.clientHeight || 1;
            let hh = 500, hw = hh * aspect;
            orthoCam.left = -hw; orthoCam.right = hw;
            orthoCam.top = hh;   orthoCam.bottom = -hh;
            orthoCam.near = cam.near; orthoCam.far = cam.far;
            orthoCam.updateProjectionMatrix();
        }

        // ── Pointer events ──────────────────────────────────────────
        domEl.addEventListener('pointerdown', function (e) {
            if (!skipGuards && (e.target.closest('float-window') || e.target.closest('.menubar'))) return;
            if (e.button === 1) {
                if (e.shiftKey) panning = true;
                else if (e.ctrlKey || e.metaKey) zooming = true;
                else orbiting = true;  // "orbiting" flag reused for look-around
            } else if (e.button === 0) { lmbDown = true; }
            else if (e.button === 2) { panning = true; }
            px = e.clientX; py = e.clientY;
            if (orbiting || panning || zooming) domEl.setPointerCapture(e.pointerId);
        });

        domEl.addEventListener('pointermove', function (e) {
            let dx = e.clientX - px, dy = e.clientY - py;
            px = e.clientX; py = e.clientY;
            if (orbiting) {
                // FPS look-around: adjust yaw/pitch directly
                hDelta -= dx * ROTATE_SPEED;
                vDelta -= dy * ROTATE_SPEED;
            }
            if (zooming) { dollyDelta -= dy / 50 * ZOOM_LINEAR; }
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

        // ── Wheel / scroll ──────────────────────────────────────────
        domEl.addEventListener('wheel', function (e) {
            if (!skipGuards && e.target.closest('float-window')) return;
            e.preventDefault();
            if (e.deltaMode === 1) { dollyDelta -= (e.deltaY > 0 ? 1 : -1) * ZOOM_LINEAR; return; }
            if (e.ctrlKey || e.metaKey) {
                dollyDelta -= e.deltaY / 100 * ZOOM_LINEAR;
            } else if (e.shiftKey || lmbDown) {
                let activeCam = getActiveCam();
                let v = new THREE.Vector3();
                v.setFromMatrixColumn(activeCam.matrix, 0); panOff.addScaledVector(v, -e.deltaX * PAN_FIXED * PAN_SPEED * 0.5);
                v.setFromMatrixColumn(activeCam.matrix, 1); panOff.addScaledVector(v, e.deltaY * PAN_FIXED * PAN_SPEED * 0.5);
            } else {
                // Default two-finger scroll → look around (FPS-style)
                hDelta += e.deltaX * ROTATE_SPEED * 0.5;
                vDelta -= e.deltaY * ROTATE_SPEED * 0.5;
            }
        }, {passive: false});

        domEl.addEventListener('contextmenu', function (e) { e.preventDefault(); });

        // ── Keyboard ────────────────────────────────────────────────
        domEl.setAttribute('tabindex', '0');
        domEl.style.outline = 'none';
        domEl.addEventListener('keydown', function (e) {
            if (!skipGuards && (e.target.closest('float-window') || e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA')) return;
            _keysDown[e.code] = true;
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
                // WASD / arrows / Q / E handled via _keysDown in update()
                case 'KeyW': case 'ArrowUp':
                case 'KeyS': case 'ArrowDown':
                case 'KeyA': case 'ArrowLeft':
                case 'KeyD': case 'ArrowRight':
                case 'KeyQ': case 'Space':
                case 'KeyE':
                    break;
                default: handled = false;
            }
            if (handled) e.preventDefault();
        });

        domEl.addEventListener('keyup', function (e) {
            delete _keysDown[e.code];
        });

        // lose all keys on blur
        domEl.addEventListener('blur', function () { _keysDown = {}; });

        // ── Animation to preset view ────────────────────────────────
        function animateTo(phi, theta) {
            // phi = polar angle from up-axis, theta = azimuthal
            // Convert spherical preset into a "look direction" and extract yaw/pitch
            let dir = new THREE.Vector3();
            let tmpSph = new THREE.Spherical(1, phi, theta);
            dir.setFromSpherical(tmpSph);
            if (zUp) { let t = dir.y; dir.y = dir.z; dir.z = t; }
            // Camera should look in -dir (orbit places camera at +dir from target)
            let lookDir = dir.clone().negate();

            let targetYaw, targetPitch;
            if (zUp) {
                targetYaw = Math.atan2(lookDir.y, lookDir.x) - Math.PI / 2;
                let hLen = Math.sqrt(lookDir.x * lookDir.x + lookDir.y * lookDir.y);
                targetPitch = Math.atan2(lookDir.z, hLen);
            } else {
                targetYaw = Math.atan2(lookDir.x, lookDir.z);
                let hLen = Math.sqrt(lookDir.x * lookDir.x + lookDir.z * lookDir.z);
                targetPitch = Math.atan2(lookDir.y, hLen);
            }

            _animFromQ.copy(_buildQuatFromYawPitch(_yaw, _pitch));
            _animToQ.copy(_buildQuatFromYawPitch(targetYaw, targetPitch));
            _animTargetYaw = targetYaw;
            _animTargetPitch = targetPitch;
            animStart = performance.now();
            animating = true;
        }

        function easeInOut(t) { return t < 0.5 ? 2*t*t : -1 + (4 - 2*t) * t; }

        // ── Axis gizmo ──────────────────────────────────────────────
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

        // ── Update ──────────────────────────────────────────────────
        let _initPos = cam.position.clone();
        let _initYaw = 0, _initPitch = 0;

        let ctrl = {
            target: new THREE.Vector3(), // kept for API compat but unused
            get maxDist() { return _maxD; },
            set maxDist(v) { _maxD = v; ZOOM_LINEAR = v * 0.05; PAN_FIXED = v / 1000; MOVE_SPEED = v * 0.5; },
            get camera() { return getActiveCam(); },

            reset: function () {
                cam.position.copy(_initPos);
                _yaw = _initYaw;
                _pitch = _initPitch;
                let q = _buildQuatFromYawPitch(_yaw, _pitch);
                cam.quaternion.copy(q);
                if (orthoCam) {
                    orthoCam.position.copy(cam.position);
                    orthoCam.quaternion.copy(cam.quaternion);
                    syncOrthoFrustum();
                }
            },

            saveInitState: function () {
                _initPos.copy(cam.position);
                if (_needsInit) _initFromCamera();
                _initYaw = _yaw;
                _initPitch = _pitch;
            },

            update: function (dt) {
                if (_needsInit) _initFromCamera();
                dt = dt || 0.016; // fallback ~60fps

                let activeCam = getActiveCam();

                if (animating) {
                    let t = Math.min(1, (performance.now() - animStart) / ANIM_DURATION);
                    t = easeInOut(t);
                    let q = new THREE.Quaternion();
                    q.slerpQuaternions(_animFromQ, _animToQ, t);
                    activeCam.quaternion.copy(q);
                    if (t >= 1) {
                        animating = false;
                        _yaw = _animTargetYaw;
                        _pitch = _animTargetPitch;
                    }
                } else {
                    // Apply mouse look deltas
                    _yaw += hDelta;
                    _pitch += vDelta;
                    // Clamp pitch to avoid flipping
                    let LIMIT = Math.PI / 2 - 0.01;
                    if (_pitch > LIMIT) _pitch = LIMIT;
                    if (_pitch < -LIMIT) _pitch = -LIMIT;

                    let q = _buildQuatFromYawPitch(_yaw, _pitch);
                    activeCam.quaternion.copy(q);
                }

                // ── WASD / arrow movement ───────────────────────────
                let speed = MOVE_SPEED * dt;
                if (_keysDown['ShiftLeft'] || _keysDown['ShiftRight']) speed *= 3;

                // Forward direction (projected onto horizontal plane for WASD)
                let fwd = new THREE.Vector3(0, 0, -1).applyQuaternion(activeCam.quaternion);
                let right = new THREE.Vector3(1, 0, 0).applyQuaternion(activeCam.quaternion);

                // Project forward onto horizontal plane for ground-based movement
                let fwdH = fwd.clone();
                if (zUp) { fwdH.z = 0; } else { fwdH.y = 0; }
                if (fwdH.lengthSq() > 1e-6) fwdH.normalize(); else fwdH.copy(fwd).normalize();

                let rightH = right.clone();
                if (zUp) { rightH.z = 0; } else { rightH.y = 0; }
                if (rightH.lengthSq() > 1e-6) rightH.normalize(); else rightH.copy(right).normalize();

                let moveVec = new THREE.Vector3();

                if (_keysDown['KeyW'] || _keysDown['ArrowUp'])    moveVec.addScaledVector(fwdH,  speed);
                if (_keysDown['KeyS'] || _keysDown['ArrowDown'])  moveVec.addScaledVector(fwdH, -speed);
                if (_keysDown['KeyA'] || _keysDown['ArrowLeft'])  moveVec.addScaledVector(rightH, -speed);
                if (_keysDown['KeyD'] || _keysDown['ArrowRight']) moveVec.addScaledVector(rightH,  speed);
                if (_keysDown['KeyQ'] || _keysDown['Space'])      moveVec.addScaledVector(_worldUp,  speed);
                if (_keysDown['KeyE'])                             moveVec.addScaledVector(_worldUp, -speed);

                activeCam.position.add(moveVec);

                // ── Dolly (scroll) — move along view direction ──────
                if (dollyDelta !== 0) {
                    _tmpV.set(0, 0, -1).applyQuaternion(activeCam.quaternion);
                    activeCam.position.addScaledVector(_tmpV, dollyDelta);
                }

                // ── Pan offset ──────────────────────────────────────
                activeCam.position.add(panOff);

                activeCam.updateMatrixWorld(true);

                if (!isPerspective) syncOrthoFrustum();
                if (isPerspective && orthoCam) { orthoCam.position.copy(cam.position); orthoCam.quaternion.copy(cam.quaternion); }
                else if (!isPerspective) { cam.position.copy(orthoCam.position); cam.quaternion.copy(orthoCam.quaternion); }

                hDelta = 0; vDelta = 0;
                panOff.set(0, 0, 0);
                dollyDelta = 0;

                if (gizmoRenderer) {
                    gizmoCamera.quaternion.copy(activeCam.quaternion);
                    gizmoCamera.position.set(0, 0, 5).applyQuaternion(activeCam.quaternion);
                    gizmoRenderer.render(gizmoScene, gizmoCamera);
                }
            }
        };
        return ctrl;
    }

    return { makeFpsControls: makeFpsControls };
})();

