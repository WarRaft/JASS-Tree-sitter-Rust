'use strict';

// ── Embedded MDX model viewer ───────────────────────────────────────

window._W3E_MODEL_VIEWER = (function () {

    function init() {
        const win = document.getElementById('modelViewerWindow');
        const container = document.getElementById('modelCanvasContainer');
        const canvas = document.getElementById('modelCanvas');
        const infoEl = document.getElementById('modelInfo');
        const nameEl = document.getElementById('modelName');
        if (!win || !container || !canvas) return {load() {}, showUnsupported() {}};

        const renderer = new THREE.WebGLRenderer({canvas, antialias: true, alpha: false});
        renderer.setPixelRatio(window.devicePixelRatio);
        renderer.setClearColor(0x1e1e1e);

        const scene = new THREE.Scene();
        const camera = new THREE.PerspectiveCamera(45, 1, 0.1, 10000);
        camera.position.set(300, 200, 300);
        camera.lookAt(0, 50, 0);

        scene.add(new THREE.AmbientLight(0x606060));
        const dirLight = new THREE.DirectionalLight(0xffffff, 0.8);
        dirLight.position.set(200, 400, 300);
        scene.add(dirLight);
        const dirLight2 = new THREE.DirectionalLight(0x4488ff, 0.3);
        dirLight2.position.set(-200, 100, -300);
        scene.add(dirLight2);

        const gridHelper = new THREE.GridHelper(500, 20, 0x444444, 0x333333);
        scene.add(gridHelper);
        const axesHelper = new THREE.AxesHelper(100);
        scene.add(axesHelper);

        const COLORS = [
            0x4fc3f7, 0xab47bc, 0x66bb6a, 0xffa726,
            0xef5350, 0x26c6da, 0xd4e157, 0xec407a,
        ];

        const rootGroup = new THREE.Group();
        rootGroup.rotation.x = -Math.PI / 2;
        scene.add(rootGroup);

        const meshGroup = new THREE.Group();
        const wireframeGroup = new THREE.Group();
        const skeletonGroup = new THREE.Group();
        rootGroup.add(meshGroup);
        rootGroup.add(wireframeGroup);
        rootGroup.add(skeletonGroup);

        let defaultCamTarget = new THREE.Vector3();
        let maxDim = 100;

        let ctrl = window._W3E_ORBIT.makeOrbitControls(camera, canvas, maxDim, {skipGuards: true});

        // Toolbar buttons
        const wireBtn = document.getElementById('mvWireBtn');
        const axesBtn = document.getElementById('mvAxesBtn');
        const gridBtn = document.getElementById('mvGridBtn');
        const resetBtn = document.getElementById('mvResetCamera');
        const geosetBtn = document.getElementById('mvGeosetBtn');
        const geosetsPanel = document.getElementById('mvGeosetsWindow');
        const geosetList = document.getElementById('mvGeosetList');
        const materialBtn = document.getElementById('mvMaterialBtn');
        const materialsPanel = document.getElementById('mvMaterialsWindow');
        const materialList = document.getElementById('mvMaterialList');
        const bonesBtn = document.getElementById('mvBonesBtn');
        const bonesPanel = document.getElementById('mvBonesWindow');
        const bonesList = document.getElementById('mvBonesList');
        const animBtn = document.getElementById('mvAnimBtn');
        const animPanel = document.getElementById('mvAnimWindow');
        const animList = document.getElementById('mvAnimList');
        const skeletonBtn = document.getElementById('mvSkeletonBtn');
        const teamColorPicker = document.getElementById('mvTeamColorPicker');

        let wireOn = false, axesOn = true, gridOn = true, skeletonOn = false;
        // Track texture indices for team color (replaceable_id=1) and team glow (replaceable_id=2)
        let teamColorTexIndices = [];
        let teamGlowTexIndices = [];
        let teamColorTexture = null;
        let teamGlowTexture = null;
        let loadedTextures = [];
        let currentMaterials = [];

        function toggleSbBtn(btn, on) {
            if (on) btn.classList.add('active');
            else btn.classList.remove('active');
        }

        if (wireBtn) wireBtn.addEventListener('click', function () {
            wireOn = !wireOn;
            toggleSbBtn(wireBtn, wireOn);
            wireframeGroup.children.forEach(function (m) {
                // Check if any mesh of the same geoset is visible
                let geoIdx = m.userData && m.userData.geoIndex;
                if (geoIdx === undefined) {
                    // wireframeGroup has one wireframe per geoset-index (creation order)
                    m.visible = wireOn;
                } else {
                    let anyVisible = meshGroup.children.some(function (mm) { return mm.userData.geoIndex === geoIdx && mm.visible; });
                    m.visible = wireOn && anyVisible;
                }
            });
        });
        if (axesBtn) axesBtn.addEventListener('click', function () {
            axesOn = !axesOn;
            toggleSbBtn(axesBtn, axesOn);
            axesHelper.visible = axesOn;
        });
        if (gridBtn) gridBtn.addEventListener('click', function () {
            gridOn = !gridOn;
            toggleSbBtn(gridBtn, gridOn);
            gridHelper.visible = gridOn;
        });
        if (resetBtn) resetBtn.addEventListener('click', function () {
            ctrl.target.copy(defaultCamTarget);
            const d2 = new THREE.Vector3(maxDim * 0.7, maxDim * 0.5, maxDim * 0.7);
            camera.position.copy(defaultCamTarget).add(d2);
            camera.lookAt(defaultCamTarget);
        });

        // Panel toggles — each is now an independent child float-window
        if (geosetBtn && geosetsPanel) {
            geosetBtn.addEventListener('click', function () {
                geosetsPanel.toggle();
                toggleSbBtn(geosetBtn, geosetsPanel.open);
            });
        }
        if (materialBtn && materialsPanel) {
            materialBtn.addEventListener('click', function () {
                materialsPanel.toggle();
                toggleSbBtn(materialBtn, materialsPanel.open);
            });
        }
        if (bonesBtn && bonesPanel) {
            bonesBtn.addEventListener('click', function () {
                bonesPanel.toggle();
                toggleSbBtn(bonesBtn, bonesPanel.open);
            });
        }
        if (animBtn && animPanel) {
            animBtn.addEventListener('click', function () {
                animPanel.toggle();
                toggleSbBtn(animBtn, animPanel.open);
            });
        }

        // Sync sidebar button state when child windows are closed via their × button
        document.addEventListener('float-toggled', function (evt) {
            let id = evt.detail && evt.detail.id;
            if (id === 'mvGeosetsWindow' && geosetBtn && geosetsPanel) {
                toggleSbBtn(geosetBtn, geosetsPanel.open);
            } else if (id === 'mvMaterialsWindow' && materialBtn && materialsPanel) {
                toggleSbBtn(materialBtn, materialsPanel.open);
            } else if (id === 'mvBonesWindow' && bonesBtn && bonesPanel) {
                toggleSbBtn(bonesBtn, bonesPanel.open);
            } else if (id === 'mvAnimWindow' && animBtn && animPanel) {
                toggleSbBtn(animBtn, animPanel.open);
            }
        });
        if (skeletonBtn) {
            skeletonBtn.addEventListener('click', function () {
                skeletonOn = !skeletonOn;
                toggleSbBtn(skeletonBtn, skeletonOn);
                skeletonGroup.children.forEach(function (c) { c.visible = skeletonOn; });
            });
        }

        // ── Team Color / Team Glow texture generation (matches MdlVis) ──

        // TeamGlowAlpha 32×32 lookup from MdlVis glow.pas
        const TEAM_GLOW_ALPHA = [
            1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,1,
            1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,
            1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,1,
            1,1,1,1,1,1,1,1,2,2,2,2,3,3,3,3,3,3,3,2,2,2,1,1,1,1,0,0,0,0,0,1,
            1,1,1,1,1,1,1,1,1,2,2,3,4,5,6,6,6,6,5,4,3,2,2,1,2,2,1,0,0,0,0,0,
            1,1,1,1,1,1,1,1,1,1,3,4,6,7,9,9,10,9,8,7,5,3,2,1,3,2,2,1,0,0,0,0,
            1,1,1,1,1,1,1,1,3,4,6,8,10,13,14,15,17,16,15,12,10,7,6,5,4,3,2,1,0,0,0,0,
            1,1,1,1,1,1,1,1,7,8,10,13,16,18,20,22,24,23,21,18,15,12,10,9,4,3,2,1,0,0,0,0,
            0,0,1,1,0,1,3,4,5,9,15,20,25,30,35,38,38,36,34,31,26,20,13,9,9,6,2,1,0,1,1,0,
            0,0,1,1,0,1,3,5,10,15,21,28,35,41,47,50,51,49,46,41,36,28,20,15,10,7,3,1,0,1,1,0,
            0,0,1,1,1,2,4,7,15,20,28,38,47,55,62,67,69,67,62,56,47,37,28,21,12,9,4,1,1,1,1,0,
            0,0,1,1,1,3,6,9,16,23,33,45,57,68,78,83,87,83,77,69,58,45,33,25,15,11,6,2,1,1,1,0,
            0,0,1,1,1,4,8,11,19,27,39,53,67,81,92,99,103,99,91,81,68,53,39,30,18,13,7,3,1,1,1,1,
            0,0,1,0,1,5,9,13,24,32,46,61,77,92,105,112,116,112,104,93,78,61,45,35,20,16,9,4,1,1,1,1,
            0,0,0,0,2,5,11,14,27,36,50,67,84,100,113,120,124,120,112,100,84,66,49,39,23,17,10,4,1,1,1,1,
            0,0,0,0,2,6,11,15,28,36,51,68,85,102,115,123,127,122,114,102,86,67,50,40,24,18,11,4,1,1,1,1,
            1,1,1,1,2,5,11,15,25,36,51,67,82,97,112,121,123,118,110,98,83,66,49,39,22,17,10,4,2,1,0,0,
            1,1,1,1,2,5,10,14,24,34,48,63,77,90,104,113,116,111,103,92,78,61,46,36,20,16,9,4,1,1,0,0,
            1,1,1,1,1,4,9,12,22,30,43,56,68,80,92,99,104,99,92,82,69,54,39,30,18,14,8,3,1,1,0,0,
            1,1,1,1,1,3,7,10,18,25,35,47,58,69,78,84,88,84,78,69,58,45,33,25,16,12,6,3,1,1,0,0,
            0,1,1,1,1,2,5,8,13,18,27,37,47,56,64,68,70,67,62,55,47,37,26,20,12,9,5,2,1,1,0,0,
            0,1,1,1,0,1,4,6,9,13,19,27,36,43,48,51,52,50,46,41,35,28,20,14,10,7,3,1,1,1,0,0,
            0,1,1,1,0,1,3,4,7,9,13,19,25,30,33,34,36,34,32,29,25,20,14,9,8,5,2,1,1,1,0,0,
            0,1,1,1,0,0,2,4,6,7,9,13,18,21,23,23,27,25,23,21,19,15,9,6,6,4,2,0,0,1,0,0,
            1,1,1,1,1,1,1,1,4,5,6,8,10,12,13,14,16,15,14,12,10,8,7,6,1,1,1,1,1,1,1,1,
            1,1,1,1,1,1,1,1,2,3,4,6,7,8,10,10,10,9,8,7,5,4,3,2,1,1,1,1,1,1,1,1,
            1,1,1,1,1,1,1,1,1,1,2,2,3,4,5,5,5,4,4,3,2,1,1,0,1,1,1,1,1,1,1,1,
            1,1,1,1,1,1,1,1,0,0,0,1,1,1,2,2,3,3,2,2,2,2,1,1,1,1,1,1,1,1,1,1,
            1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,0,1,1,1,1,1,2,2,2,1,1,1,1,1,1,1,1,
            1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,
            1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,
            1,1,1,1,1,1,1,1,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,0,1,1,1,1,1,1,1,1,
        ];

        function hexToRgb01(hex) {
            let r = parseInt(hex.slice(1, 3), 16) / 255;
            let g = parseInt(hex.slice(3, 5), 16) / 255;
            let b = parseInt(hex.slice(5, 7), 16) / 255;
            return [r, g, b];
        }

        // Team Color (replaceableId=1): 8×8 solid color, full alpha (MdlVis)
        function makeTeamColorTexture(hex) {
            let c = document.createElement('canvas');
            c.width = 8;
            c.height = 8;
            let ctx = c.getContext('2d');
            ctx.fillStyle = hex;
            ctx.fillRect(0, 0, 8, 8);
            let tex = new THREE.CanvasTexture(c);
            tex.wrapS = THREE.RepeatWrapping;
            tex.wrapT = THREE.RepeatWrapping;
            tex.magFilter = THREE.LinearFilter;
            tex.minFilter = THREE.LinearFilter;
            tex.needsUpdate = true;
            return tex;
        }

        // Team Glow (replaceableId=2): 32×32 pre-multiplied alpha glow (MdlVis)
        function makeTeamGlowTexture(hex) {
            let rgb = hexToRgb01(hex);
            let c = document.createElement('canvas');
            c.width = 32;
            c.height = 32;
            let ctx = c.getContext('2d');
            let imgData = ctx.createImageData(32, 32);
            let d = imgData.data;
            for (let i = 0; i < 32 * 32; i++) {
                let a = TEAM_GLOW_ALPHA[i]; // 0..127
                let off = i * 4;
                d[off]     = Math.round(rgb[0] * a * 2); // scale to 0..254
                d[off + 1] = Math.round(rgb[1] * a * 2);
                d[off + 2] = Math.round(rgb[2] * a * 2);
                d[off + 3] = Math.min(255, a * 2);       // alpha scaled to 0..254
            }
            ctx.putImageData(imgData, 0, 0);
            let tex = new THREE.CanvasTexture(c);
            tex.wrapS = THREE.RepeatWrapping;
            tex.wrapT = THREE.RepeatWrapping;
            tex.magFilter = THREE.LinearFilter;
            tex.minFilter = THREE.LinearFilter;
            tex.premultiplyAlpha = true;
            tex.needsUpdate = true;
            return tex;
        }

        function applyTeamColor(hex) {
            if (teamColorTexIndices.length === 0 && teamGlowTexIndices.length === 0) return;
            if (teamColorTexture) teamColorTexture.dispose();
            if (teamGlowTexture) teamGlowTexture.dispose();
            teamColorTexture = makeTeamColorTexture(hex);
            teamGlowTexture = makeTeamGlowTexture(hex);
            teamColorTexIndices.forEach(function (ti) { loadedTextures[ti] = teamColorTexture; });
            teamGlowTexIndices.forEach(function (ti) { loadedTextures[ti] = teamGlowTexture; });
            let allTeamIndices = teamColorTexIndices.concat(teamGlowTexIndices);
            meshGroup.children.forEach(function (m) {
                let matId = m.userData.materialId;
                let li = m.userData.layerIndex || 0;
                let mat = currentMaterials[matId];
                let layers = mat ? (mat.layers || []) : [];
                let layer = layers[li];
                if (layer && allTeamIndices.indexOf(layer.texture_id) >= 0) {
                    let isGlow = teamGlowTexIndices.indexOf(layer.texture_id) >= 0;
                    m.material.map = isGlow ? teamGlowTexture : teamColorTexture;
                    m.material.color.set(0xffffff);
                    m.material.needsUpdate = true;
                }
            });
        }

        if (teamColorPicker) {
            teamColorPicker.addEventListener('input', function () {
                applyTeamColor(teamColorPicker.value);
            });
        }

        // ── Animation state ──────────────────────────────────────────────
        let currentSequences = [];
        let globalSequences = [];
        let allNodes = [];          // bones + helpers merged, indexed by object_id
        let pivotPoints = [];
        let geosetSkinData = [];    // per-geoset vertex group + matrix mapping
        let geosetAnims = [];       // parsed GEOA data (per-geoset alpha/color anim)
        let activeAnimIndex = -1;
        let animPlaying = false;
        let animFrame = 0;
        let animStartTime = 0;      // wallclock start (for global sequences)

        // Bone world transforms cache
        let boneWorldMatrices = [];  // array of THREE.Matrix4, indexed by object_id

        // Saved rest-pose vertices per geoset (Float32Array copies)
        let restPoseVertices = [];

        // ── Keyframe interpolation helpers ─────────────────────────────
        function interpLinear(a, b, t) {
            let out = [];
            for (let i = 0; i < a.length; i++) out[i] = a[i] + (b[i] - a[i]) * t;
            return out;
        }
        function interpHermite(a, b, aOut, bIn, t) {
            let s2 = t * t, s3 = s2 * t;
            let h1 = 2 * s3 - 3 * s2 + 1;
            let h2 = -2 * s3 + 3 * s2;
            let h3 = s3 - 2 * s2 + t;
            let h4 = s3 - s2;
            let out = [];
            for (let i = 0; i < a.length; i++) {
                out[i] = h1 * a[i] + h2 * b[i] + h3 * aOut[i] + h4 * bIn[i];
            }
            return out;
        }
        function slerpQuat(qa, qb, t) {
            let ax = qa[0], ay = qa[1], az = qa[2], aw = qa[3];
            let bx = qb[0], by = qb[1], bz = qb[2], bw = qb[3];
            let dot = ax * bx + ay * by + az * bz + aw * bw;
            if (dot < 0) { bx = -bx; by = -by; bz = -bz; bw = -bw; dot = -dot; }
            if (dot > 0.9995) {
                return [ax + t * (bx - ax), ay + t * (by - ay), az + t * (bz - az), aw + t * (bw - aw)];
            }
            let theta = Math.acos(Math.min(dot, 1));
            let sinT = Math.sin(theta);
            let s0 = Math.sin((1 - t) * theta) / sinT;
            let s1 = Math.sin(t * theta) / sinT;
            return [s0 * ax + s1 * bx, s0 * ay + s1 * by, s0 * az + s1 * bz, s0 * aw + s1 * bw];
        }

        function evalTrack(track, frame, isQuat) {
            if (!track || !track.keyframes || track.keyframes.length === 0) return null;
            let kfs = track.keyframes;

            // Global sequence: wrap frame into [0..duration) using wallclock time
            let actualFrame = frame;
            if (track.global_seq_id >= 0 && track.global_seq_id < globalSequences.length) {
                let gsDuration = globalSequences[track.global_seq_id];
                if (gsDuration <= 0) return kfs[0].value.slice();
                // Use wallclock elapsed since animation started
                let elapsed = (performance.now() - animStartTime);
                actualFrame = elapsed % gsDuration;
            }

            if (kfs.length === 1) return kfs[0].value.slice();
            // Clamp
            if (actualFrame <= kfs[0].frame) return kfs[0].value.slice();
            if (actualFrame >= kfs[kfs.length - 1].frame) return kfs[kfs.length - 1].value.slice();
            // Find enclosing keyframes
            let lo = 0;
            for (let i = 0; i < kfs.length - 1; i++) {
                if (kfs[i].frame <= actualFrame && kfs[i + 1].frame > actualFrame) { lo = i; break; }
            }
            let k0 = kfs[lo], k1 = kfs[lo + 1];
            let span = k1.frame - k0.frame;
            let t = span > 0 ? (actualFrame - k0.frame) / span : 0;
            if (track.line_type === 0) return k0.value.slice(); // DontInterp
            if (isQuat) return slerpQuat(k0.value, k1.value, t);
            if (track.line_type >= 2 && k0.out_tan && k0.out_tan.length && k1.in_tan && k1.in_tan.length) {
                return interpHermite(k0.value, k1.value, k0.out_tan, k1.in_tan, t);
            }
            return interpLinear(k0.value, k1.value, t);
        }

        function computeBoneMatrices(frame) {
            let identQuat = [0, 0, 0, 1];
            let identTrans = [0, 0, 0];
            let identScale = [1, 1, 1];
            // Reset ready flags
            let ready = new Array(allNodes.length).fill(false);

            function computeNode(idx) {
                if (idx < 0 || idx >= allNodes.length) return;
                if (ready[idx]) return;
                let node = allNodes[idx];
                if (!node) { ready[idx] = true; return; }

                let parentIdx = (node.parent_id === 0xFFFFFFFF || node.parent_id === 4294967295) ? -1 : node.parent_id;
                if (parentIdx >= 0 && !ready[parentIdx]) computeNode(parentIdx);

                let tr = evalTrack(node.translation, frame, false) || identTrans;
                let rot = evalTrack(node.rotation, frame, true) || identQuat;
                let sc = evalTrack(node.scaling, frame, false) || identScale;

                let pivot = pivotPoints[idx] || [0, 0, 0];

                let localMat = new THREE.Matrix4();
                let q = new THREE.Quaternion(rot[0], rot[1], rot[2], rot[3]);
                let s = new THREE.Vector3(sc[0], sc[1], sc[2]);
                let p = new THREE.Vector3(
                    pivot[0] + tr[0],
                    pivot[1] + tr[1],
                    pivot[2] + tr[2]
                );
                localMat.compose(p, q, s);

                // Subtract pivot before rotation then add it back (pivot-based transform)
                // Actually MDX does: M = T(pivot) * T(translation) * R * S * T(-pivot)
                // But simpler approach: compose at pivot
                let pivotMat = new THREE.Matrix4();
                pivotMat.makeTranslation(-pivot[0], -pivot[1], -pivot[2]);
                let fullLocal = new THREE.Matrix4();
                fullLocal.makeTranslation(pivot[0] + tr[0], pivot[1] + tr[1], pivot[2] + tr[2]);
                let rotMat = new THREE.Matrix4().makeRotationFromQuaternion(q);
                let scaleMat = new THREE.Matrix4().makeScale(sc[0], sc[1], sc[2]);
                fullLocal.multiply(rotMat).multiply(scaleMat).multiply(pivotMat);

                if (parentIdx >= 0 && boneWorldMatrices[parentIdx]) {
                    boneWorldMatrices[idx] = new THREE.Matrix4().multiplyMatrices(boneWorldMatrices[parentIdx], fullLocal);
                } else {
                    boneWorldMatrices[idx] = fullLocal;
                }
                ready[idx] = true;
            }

            for (let i = 0; i < allNodes.length; i++) computeNode(i);
        }

        function applySkinning() {
            for (let gi = 0; gi < geosetSkinData.length; gi++) {
                let skin = geosetSkinData[gi];
                if (!skin || !skin.vertexGroups || skin.vertexGroups.length === 0) continue;
                let mesh = null;
                // Find mesh for this geoset index
                for (let mi = 0; mi < meshGroup.children.length; mi++) {
                    if (meshGroup.children[mi].userData.geoIndex === gi) {
                        mesh = meshGroup.children[mi];
                        break;
                    }
                }
                if (!mesh) continue;
                let posAttr = mesh.geometry.getAttribute('position');
                if (!posAttr) continue;
                let restVerts = restPoseVertices[gi];
                if (!restVerts) continue;

                let groups = skin.vertexGroups;
                let matIds = skin.matrixIds;
                let matCounts = skin.matrixGroupCounts;

                // Build offset lookup: group index → start offset in matIds
                let groupOffset = [];
                let off = 0;
                for (let g = 0; g < matCounts.length; g++) {
                    groupOffset[g] = off;
                    off += matCounts[g];
                }

                let vec = new THREE.Vector3();
                let tmpVec = new THREE.Vector3();
                for (let vi = 0; vi < posAttr.count; vi++) {
                    let grp = groups[vi] || 0;
                    if (grp >= matCounts.length) {
                        // No mapping, keep rest pose
                        continue;
                    }
                    let start = groupOffset[grp];
                    let count = matCounts[grp];
                    if (count === 0) continue;

                    vec.set(0, 0, 0);
                    let rx = restVerts[vi * 3], ry = restVerts[vi * 3 + 1], rz = restVerts[vi * 3 + 2];
                    for (let bi = 0; bi < count; bi++) {
                        let boneId = matIds[start + bi];
                        let mat = boneWorldMatrices[boneId];
                        if (!mat) { vec.set(rx, ry, rz); break; }
                        tmpVec.set(rx, ry, rz).applyMatrix4(mat);
                        vec.x += tmpVec.x;
                        vec.y += tmpVec.y;
                        vec.z += tmpVec.z;
                    }
                    if (count > 1) {
                        vec.x /= count;
                        vec.y /= count;
                        vec.z /= count;
                    }
                    posAttr.setXYZ(vi, vec.x, vec.y, vec.z);
                }
                posAttr.needsUpdate = true;
                mesh.geometry.computeBoundingSphere();
            }
        }

        // Update skeleton bone lines & spheres to match animated bone positions
        function updateSkeleton() {
            if (skeletonGroup.children.length === 0) return;
            // First child = LineSegments (bone lines), rest = spheres at pivots
            let skelLines = null;
            let spheres = [];
            for (let i = 0; i < skeletonGroup.children.length; i++) {
                let c = skeletonGroup.children[i];
                if (c.isLineSegments) skelLines = c;
                else if (c.isMesh) spheres.push(c);
            }

            // Update bone line positions
            if (skelLines && skelLines.geometry) {
                let posAttr = skelLines.geometry.getAttribute('position');
                if (posAttr && skelLines.userData.skelNodes) {
                    let nodes = skelLines.userData.skelNodes;
                    let vi = 0;
                    for (let ni = 0; ni < nodes.length; ni++) {
                        let node = nodes[ni];
                        if (node.parentId === 0xFFFFFFFF || node.parentId === 4294967295) continue;
                        // Compute animated world positions
                        let childPivot = pivotPoints[node.objectId];
                        let parentPivot = pivotPoints[node.parentId];
                        if (!childPivot || !parentPivot) continue;

                        let pPos = new THREE.Vector3(parentPivot[0], parentPivot[1], parentPivot[2]);
                        let cPos = new THREE.Vector3(childPivot[0], childPivot[1], childPivot[2]);
                        if (boneWorldMatrices[node.parentId]) pPos.applyMatrix4(boneWorldMatrices[node.parentId]);
                        if (boneWorldMatrices[node.objectId]) cPos.applyMatrix4(boneWorldMatrices[node.objectId]);

                        posAttr.setXYZ(vi, pPos.x, pPos.y, pPos.z);
                        posAttr.setXYZ(vi + 1, cPos.x, cPos.y, cPos.z);
                        vi += 2;
                    }
                    posAttr.needsUpdate = true;
                }
            }

            // Update sphere positions
            for (let si = 0; si < spheres.length; si++) {
                let sphere = spheres[si];
                let objId = sphere.userData.objectId;
                if (objId === undefined) continue;
                let pivot = pivotPoints[objId];
                if (!pivot) continue;
                let pos = new THREE.Vector3(pivot[0], pivot[1], pivot[2]);
                if (boneWorldMatrices[objId]) pos.applyMatrix4(boneWorldMatrices[objId]);
                sphere.position.set(pos.x, pos.y, pos.z);
            }
        }

        // Evaluate per-geoset alpha from GEOA animation
        function applyGeosetAnims(frame) {
            if (!geosetAnims || geosetAnims.length === 0) return;
            for (let gi = 0; gi < geosetAnims.length; gi++) {
                let ga = geosetAnims[gi];
                if (!ga) continue;
                let alpha = 1.0;
                if (ga.alpha_track) {
                    let val = evalTrack(ga.alpha_track, frame, false);
                    if (val) alpha = val[0];
                }
                // Apply alpha to all meshes for this geoset
                meshGroup.children.forEach(function (m) {
                    if (m.userData.geoIndex === ga.geoset_id) {
                        if (alpha < 1.0) {
                            m.material.transparent = true;
                            m.material.opacity = alpha;
                        } else {
                            // Restore original opacity if layer doesn't need transparency
                            let layer = null;
                            let li = m.userData.layerIndex || 0;
                            let mat = currentMaterials[m.userData.materialId];
                            if (mat && mat.layers) layer = mat.layers[li];
                            let fm = layer ? layer.filter_mode : 0;
                            if (fm === 0 && !(layer && layer.alpha < 1.0)) {
                                m.material.transparent = false;
                                m.material.opacity = 1.0;
                            } else {
                                m.material.opacity = layer ? layer.alpha : 1.0;
                            }
                        }
                        m.material.needsUpdate = true;
                    }
                });
            }
        }

        function updateAnimation(dt) {
            if (activeAnimIndex < 0 || !animPlaying) return;
            let seq = currentSequences[activeAnimIndex];
            if (!seq) return;
            let start = seq.interval_start;
            let end = seq.interval_end;
            let duration = end - start;
            if (duration <= 0) return;

            animFrame += dt;
            if (animFrame > end) {
                if (seq.non_looping) {
                    animFrame = end;
                    animPlaying = false;
                    // Update play button
                    let btn = animList ? animList.querySelector('.mv-anim-play-btn.playing') : null;
                    if (btn) { btn.classList.remove('playing'); btn.textContent = '\u25b6'; }
                } else {
                    animFrame = start + ((animFrame - start) % duration);
                }
            }

            computeBoneMatrices(animFrame);
            applySkinning();
            if (skeletonOn) updateSkeleton();
            applyGeosetAnims(animFrame);

            // Update slider
            let slider = animList ? animList.querySelector('.mv-anim-item[data-index="' + activeAnimIndex + '"] .mv-anim-slider') : null;
            if (slider) slider.value = animFrame;
            let label = animList ? animList.querySelector('.mv-anim-item[data-index="' + activeAnimIndex + '"] .mv-anim-frame-label') : null;
            if (label) label.textContent = Math.round(animFrame) + ' / ' + end;
        }

        function setAnimSequence(index) {
            if (index < 0 || index >= currentSequences.length) return;
            // Reset previous active
            if (animList) {
                let prev = animList.querySelector('.mv-anim-active');
                if (prev) prev.classList.remove('mv-anim-active');
                let prevBtn = animList.querySelector('.mv-anim-play-btn.playing');
                if (prevBtn) { prevBtn.classList.remove('playing'); prevBtn.textContent = '\u25b6'; }
            }
            activeAnimIndex = index;
            let seq = currentSequences[index];
            animFrame = seq.interval_start;
            animPlaying = true;
            animStartTime = performance.now();

            let item = animList ? animList.querySelector('.mv-anim-item[data-index="' + index + '"]') : null;
            if (item) {
                item.classList.add('mv-anim-active');
                let btn = item.querySelector('.mv-anim-play-btn');
                if (btn) { btn.classList.add('playing'); btn.textContent = '\u23f8'; }
                let slider = item.querySelector('.mv-anim-slider');
                if (slider) slider.value = animFrame;
            }
        }

        function resetToRestPose() {
            for (let gi = 0; gi < restPoseVertices.length; gi++) {
                let rest = restPoseVertices[gi];
                if (!rest) continue;
                let mesh = null;
                for (let mi = 0; mi < meshGroup.children.length; mi++) {
                    if (meshGroup.children[mi].userData.geoIndex === gi) {
                        mesh = meshGroup.children[mi];
                        break;
                    }
                }
                if (!mesh) continue;
                let posAttr = mesh.geometry.getAttribute('position');
                if (!posAttr) continue;
                for (let vi = 0; vi < posAttr.count; vi++) {
                    posAttr.setXYZ(vi, rest[vi * 3], rest[vi * 3 + 1], rest[vi * 3 + 2]);
                }
                posAttr.needsUpdate = true;
            }
        }

        function buildAnimUI(sequences) {
            if (!animList) return;
            animList.innerHTML = '';
            if (!sequences || sequences.length === 0) {
                animList.innerHTML = '<div class="mv-anim-empty">No animations</div>';
                return;
            }
            sequences.forEach(function (seq, i) {
                let item = document.createElement('div');
                item.className = 'mv-anim-item';
                item.setAttribute('data-index', i);
                let dur = seq.interval_end - seq.interval_start;

                let header = document.createElement('div');
                header.className = 'mv-anim-header';

                let playBtn = document.createElement('button');
                playBtn.className = 'mv-anim-play-btn';
                playBtn.textContent = '\u25b6';
                playBtn.title = 'Play / Pause';
                playBtn.addEventListener('click', function (e) {
                    e.stopPropagation();
                    if (activeAnimIndex === i && animPlaying) {
                        animPlaying = false;
                        playBtn.classList.remove('playing');
                        playBtn.textContent = '\u25b6';
                    } else if (activeAnimIndex === i && !animPlaying) {
                        animPlaying = true;
                        playBtn.classList.add('playing');
                        playBtn.textContent = '\u23f8';
                    } else {
                        setAnimSequence(i);
                    }
                });

                let nameSpan = document.createElement('span');
                nameSpan.className = 'mv-anim-name';
                nameSpan.textContent = seq.name || ('Sequence ' + i);
                nameSpan.title = seq.name;
                nameSpan.addEventListener('click', function () {
                    setAnimSequence(i);
                });

                let durSpan = document.createElement('span');
                durSpan.className = 'mv-anim-duration';
                durSpan.textContent = dur + ' ms';

                let flags = document.createElement('span');
                flags.className = 'mv-anim-flags';
                if (seq.non_looping) {
                    let f = document.createElement('span');
                    f.className = 'mv-anim-flag';
                    f.textContent = 'once';
                    flags.appendChild(f);
                }

                header.appendChild(playBtn);
                header.appendChild(nameSpan);
                header.appendChild(flags);
                header.appendChild(durSpan);

                let sliderRow = document.createElement('div');
                sliderRow.className = 'mv-anim-slider-row';

                let slider = document.createElement('input');
                slider.type = 'range';
                slider.className = 'mv-anim-slider';
                slider.min = seq.interval_start;
                slider.max = seq.interval_end;
                slider.step = 1;
                slider.value = seq.interval_start;
                slider.addEventListener('input', function () {
                    if (activeAnimIndex !== i) {
                        setAnimSequence(i);
                    }
                    animPlaying = false;
                    let btn2 = item.querySelector('.mv-anim-play-btn');
                    if (btn2) { btn2.classList.remove('playing'); btn2.textContent = '\u25b6'; }
                    animFrame = parseFloat(slider.value);
                    computeBoneMatrices(animFrame);
                    applySkinning();
                    if (skeletonOn) updateSkeleton();
                    applyGeosetAnims(animFrame);
                    if (frameLabel) frameLabel.textContent = Math.round(animFrame) + ' / ' + seq.interval_end;
                });

                let frameLabel = document.createElement('span');
                frameLabel.className = 'mv-anim-frame-label';
                frameLabel.textContent = seq.interval_start + ' / ' + seq.interval_end;

                sliderRow.appendChild(slider);
                sliderRow.appendChild(frameLabel);

                item.appendChild(header);
                item.appendChild(sliderRow);
                animList.appendChild(item);
            });
        }


        // Resize
        function onResize() {
            const w = container.clientWidth;
            const h = container.clientHeight;
            if (w === 0 || h === 0) return;
            renderer.setSize(w, h);
            camera.aspect = w / h;
            camera.updateProjectionMatrix();
        }
        const resizeObs = new ResizeObserver(onResize);
        resizeObs.observe(container);

        // Animation loop
        let animating = false;
        let lastFrameTime = 0;
        function animate(now) {
            if (!animating) return;
            requestAnimationFrame(animate);
            let dt = lastFrameTime ? (now - lastFrameTime) : 0;
            lastFrameTime = now;
            if (animPlaying && activeAnimIndex >= 0) {
                updateAnimation(dt);
            }
            ctrl.update();
            renderer.render(scene, ctrl.camera);
        }

        new MutationObserver(function () {
            if (win.open) {
                animating = true;
                onResize();
                animate();
            } else {
                animating = false;
            }
        }).observe(win, {attributes: true, attributeFilter: ['hidden']});

        // Geometry data arrives as TypedArrays from the binary protocol —
        // no base64 decoding needed.

        let FILTER_MODE_NAMES = [
            'None', 'Transparent', 'Blend', 'Additive',
            'AddAlpha', 'Modulate', 'Modulate2x'
        ];

        let SHADING_FLAG_BITS = [
            {bit: 0x01, name: 'Unshaded'},
            {bit: 0x02, name: 'SphereEnvMap'},
            {bit: 0x10, name: 'TwoSided'},
            {bit: 0x20, name: 'Unfogged'},
            {bit: 0x40, name: 'NoDepthTest'},
            {bit: 0x80, name: 'NoDepthSet'},
        ];

        function decodeShadingFlags(flags) {
            let names = [];
            for (let i = 0; i < SHADING_FLAG_BITS.length; i++) {
                if (flags & SHADING_FLAG_BITS[i].bit) names.push(SHADING_FLAG_BITS[i].name);
            }
            return names.length > 0 ? names.join(', ') : 'None';
        }


        function textureUrl(bs, archivePath, texPath) {
            if (!bs || !texPath) return null;
            let params = new URLSearchParams({
                token: bs.token,
                path: texPath,
            });
            if (archivePath) params.set('archive', archivePath);
            return 'http://127.0.0.1:' + bs.port + '/mdx/texture?' + params;
        }

        function load(msg) {
            meshGroup.clear();
            wireframeGroup.clear();
            skeletonGroup.clear();

            if (nameEl) nameEl.textContent = msg.name || 'Model';

            const geosets = msg.geosets || [];
            const textures = msg.textures || [];
            const materials = msg.materials || [];
            const bones = msg.bones || [];
            const helpers = msg.helpers || [];
            const attachments = msg.attachments || [];
            const pp = msg.pivot_points || [];
            const sequences = msg.sequences || [];
            const globalSeqs = msg.global_sequences || [];
            const geosetAnimations = msg.geoset_anims || [];
            const bs = msg.binaryServer || window.__W3E_DATA__.binaryServer || null;
            const archivePath = msg.archivePath || window.__W3E_DATA__.archivePath || null;
            const replaceableTextures = msg.replaceableTextures || null;

            if (geosets.length === 0) {
                if (infoEl) infoEl.textContent = 'No geosets';
                win.show();
                return;
            }

            // ── Store animation data ──
            currentSequences = sequences;
            globalSequences = globalSeqs;
            pivotPoints = pp;
            activeAnimIndex = -1;
            animPlaying = false;
            animFrame = 0;
            animStartTime = performance.now();
            restPoseVertices = [];
            geosetSkinData = [];
            geosetAnims = geosetAnimations;

            // Merge bones + helpers + attachments into allNodes indexed by object_id
            let maxObjId = 0;
            bones.forEach(function (b) { if (b.object_id > maxObjId) maxObjId = b.object_id; });
            helpers.forEach(function (h) { if (h.object_id > maxObjId) maxObjId = h.object_id; });
            attachments.forEach(function (a) { if (a.object_id > maxObjId) maxObjId = a.object_id; });
            allNodes = new Array(maxObjId + 1).fill(null);
            boneWorldMatrices = new Array(maxObjId + 1).fill(null);
            bones.forEach(function (b) { allNodes[b.object_id] = b; });
            helpers.forEach(function (h) { allNodes[h.object_id] = h; });
            attachments.forEach(function (a) { allNodes[a.object_id] = a; });

            loadedTextures = new Array(textures.length).fill(null);
            currentMaterials = materials;
            let textureLoader = new THREE.TextureLoader();
            textureLoader.crossOrigin = 'anonymous';

            // Detect team color (replaceable_id=1) and team glow (replaceable_id=2)
            teamColorTexIndices = [];
            teamGlowTexIndices = [];
            textures.forEach(function (tex, i) {
                if (!tex) return;
                if (tex.replaceable_id === 1) teamColorTexIndices.push(i);
                if (tex.replaceable_id === 2) teamGlowTexIndices.push(i);
            });

            // Apply initial team color/glow textures
            if ((teamColorTexIndices.length > 0 || teamGlowTexIndices.length > 0) && teamColorPicker) {
                let hex = teamColorPicker.value;
                teamColorTexture = makeTeamColorTexture(hex);
                teamGlowTexture = makeTeamGlowTexture(hex);
                teamColorTexIndices.forEach(function (ti) { loadedTextures[ti] = teamColorTexture; });
                teamGlowTexIndices.forEach(function (ti) { loadedTextures[ti] = teamGlowTexture; });
            }

            function getTextureForLayer(layerObj) {
                if (!layerObj) return null;
                let texId = layerObj.texture_id;
                if (texId < loadedTextures.length && loadedTextures[texId]) {
                    return {texture: loadedTextures[texId], texIndex: texId};
                }
                return null;
            }

            function getLayersForMaterial(materialId) {
                if (materialId < materials.length) {
                    let mat = materials[materialId];
                    return mat.layers || [];
                }
                return [];
            }

            // Build a mesh for a single layer of a geoset
            function buildLayerMesh(geometry, layer, geoIdx, layerIdx, materialId) {
                const color = COLORS[geoIdx % COLORS.length];
                let texInfo = getTextureForLayer(layer);
                let sf = layer ? layer.shading_flags : 0;
                let fm = layer ? layer.filter_mode : 0;

                let matOpts = { flatShading: false };

                // TwoSided (0x10) → DoubleSide
                matOpts.side = (sf & 0x10) ? THREE.DoubleSide : THREE.DoubleSide;

                // NoDepthTest (0x40)
                if (sf & 0x40) matOpts.depthTest = false;

                // NoDepthSet (0x80)
                if (sf & 0x80) matOpts.depthWrite = false;

                if (texInfo) {
                    matOpts.map = texInfo.texture;
                }

                // Blending modes matching MdlVis Real3D.pas:
                //   Pass 1: Opaque (fm=0) + ColorAlpha (fm=1) — depth write ON
                //   Pass 2: FullAlpha/Blend (fm=2) — GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA, depth write OFF
                //   Pass 3: Additive (fm=3), AddAlpha (fm=4), Modulate (fm=5,6) — GL_ONE,GL_ONE / GL_SRC_ALPHA,GL_ONE, depth write OFF
                if (fm === 0) {
                    // None/Opaque — no blending
                    matOpts.transparent = false;
                } else if (fm === 1) {
                    // Transparent/ColorAlpha — alpha test ≥ 0.75
                    matOpts.transparent = true;
                    matOpts.alphaTest = 0.75;
                } else if (fm === 2) {
                    // Blend/FullAlpha — GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA
                    matOpts.transparent = true;
                    matOpts.blending = THREE.NormalBlending;
                    matOpts.depthWrite = false;
                } else if (fm === 3) {
                    // Additive — GL_ONE, GL_ONE
                    matOpts.transparent = true;
                    matOpts.blending = THREE.AdditiveBlending;
                    matOpts.depthWrite = false;
                } else if (fm === 4) {
                    // AddAlpha — GL_SRC_ALPHA, GL_ONE
                    matOpts.transparent = true;
                    matOpts.blending = THREE.CustomBlending;
                    matOpts.blendSrc = THREE.SrcAlphaFactor;
                    matOpts.blendDst = THREE.OneFactor;
                    matOpts.depthWrite = false;
                } else if (fm === 5 || fm === 6) {
                    // Modulate / Modulate2x — GL_ONE, GL_ONE (same as Additive in MdlVis)
                    matOpts.transparent = true;
                    matOpts.blending = THREE.AdditiveBlending;
                    matOpts.depthWrite = false;
                }

                // Layer alpha
                if (layer && layer.alpha < 1.0) {
                    matOpts.transparent = true;
                    matOpts.opacity = layer.alpha;
                }

                if (!texInfo) {
                    matOpts.color = color;
                    if (!matOpts.transparent) {
                        matOpts.transparent = true;
                        matOpts.opacity = 0.95;
                    }
                }

                // Render order: opaque layers first (0,1), then blend (2), then additive (3,4,5,6)
                let renderOrder = 0;
                if (fm === 0 || fm === 1) renderOrder = 0;
                else if (fm === 2) renderOrder = 1;
                else renderOrder = 2;

                // Unshaded (0x01) → use MeshBasicMaterial (no lighting)
                let material;
                if (sf & 0x01) {
                    material = new THREE.MeshBasicMaterial(matOpts);
                } else {
                    material = new THREE.MeshPhongMaterial(matOpts);
                }
                material.userData = {hasTexture: !!texInfo, fallbackColor: color, materialId: materialId, layerIndex: layerIdx};

                const mesh = new THREE.Mesh(geometry, material);
                mesh.renderOrder = renderOrder;
                mesh.userData.geoIndex = geoIdx;
                mesh.userData.materialId = materialId;
                mesh.userData.layerIndex = layerIdx;
                return mesh;
            }

            let totalVerts = 0, totalFaces = 0;

            geosets.forEach(function (g, idx) {
                if (!g.vertex_count || !g.face_count) return;
                const vertices = g.vertices instanceof Float32Array ? g.vertices : new Float32Array(0);
                const normals = g.normals instanceof Float32Array ? g.normals : new Float32Array(0);
                const faces = g.faces instanceof Uint16Array ? g.faces : new Uint16Array(0);
                const uvs = g.uvs instanceof Float32Array ? g.uvs : new Float32Array(0);

                // Store skinning data
                let vg = g.vertex_groups instanceof Uint8Array ? g.vertex_groups : new Uint8Array(0);
                let mi = g.matrix_ids instanceof Uint32Array ? g.matrix_ids : new Uint32Array(0);
                let mc = g.matrix_group_counts instanceof Uint32Array ? g.matrix_group_counts : new Uint32Array(0);
                geosetSkinData[idx] = {
                    vertexGroups: vg,
                    matrixIds: mi,
                    matrixGroupCounts: mc,
                };
                // Save rest-pose vertices copy
                restPoseVertices[idx] = new Float32Array(vertices);

                totalVerts += g.vertex_count;
                totalFaces += g.face_count;

                const geometry = new THREE.BufferGeometry();
                geometry.setAttribute('position', new THREE.BufferAttribute(vertices, 3));
                if (normals.length > 0) geometry.setAttribute('normal', new THREE.BufferAttribute(normals, 3));
                if (uvs.length > 0) geometry.setAttribute('uv', new THREE.BufferAttribute(uvs, 2));
                geometry.setIndex(new THREE.BufferAttribute(faces, 1));
                if (normals.length === 0) geometry.computeVertexNormals();

                const layers = getLayersForMaterial(g.material_id);

                if (layers.length === 0) {
                    // No layers at all — render as fallback with geoset color
                    let mesh = buildLayerMesh(geometry, null, idx, 0, g.material_id);
                    meshGroup.add(mesh);
                } else {
                    // Render each layer as a separate pass (multi-pass rendering as in MdlVis)
                    layers.forEach(function (layer, li) {
                        let mesh = buildLayerMesh(geometry, layer, idx, li, g.material_id);
                        meshGroup.add(mesh);
                    });
                }

                const wireMat = new THREE.MeshBasicMaterial({
                    color: 0xffffff, wireframe: true, transparent: true, opacity: 0.15,
                });
                const wireMesh = new THREE.Mesh(geometry, wireMat);
                wireMesh.visible = wireOn;
                wireMesh.userData.geoIndex = idx;
                wireframeGroup.add(wireMesh);
            });

            // Load textures
            if (bs) {
                textures.forEach(function (tex, i) {
                    if (!tex) return;
                    // Skip team color/glow textures — handled by color picker
                    if (tex.replaceable_id === 1 && teamColorTexIndices.length > 0) return;
                    if (tex.replaceable_id === 2 && teamGlowTexIndices.length > 0) return;
                    let actualPath = null;
                    if (tex.replaceable_id && replaceableTextures) {
                        if (replaceableTextures._cliffTex !== undefined) {
                            actualPath = replaceableTextures._cliffTex;
                        } else if (replaceableTextures[tex.replaceable_id]) {
                            actualPath = replaceableTextures[tex.replaceable_id];
                        }
                    } else if (tex.file_name && !tex.replaceable_id) {
                        actualPath = tex.file_name;
                    }
                    if (!actualPath) return;
                    let url = textureUrl(bs, archivePath, actualPath);
                    if (!url) return;

                    let threeTex = textureLoader.load(url, function () {
                        meshGroup.children.forEach(function (m) {
                            let matId = m.userData.materialId;
                            let li = m.userData.layerIndex || 0;
                            let layers = getLayersForMaterial(matId);
                            let layer = layers[li];
                            if (layer && layer.texture_id === i) {
                                m.material.map = threeTex;
                                m.material.color.set(0xffffff);
                                m.material.needsUpdate = true;
                            }
                        });
                        let imgs = document.querySelectorAll('[data-mv-tex-index="' + i + '"]');
                        imgs.forEach(function (img) {
                            img.src = url;
                            img.style.display = '';
                        });
                    });
                    threeTex.wrapS = THREE.RepeatWrapping;
                    threeTex.wrapT = THREE.RepeatWrapping;
                    threeTex.magFilter = THREE.LinearFilter;
                    threeTex.minFilter = THREE.LinearMipmapLinearFilter;
                    loadedTextures[i] = threeTex;
                });
            }

            // Populate geosets panel
            if (geosetList) {
                geosetList.innerHTML = '';
                geosets.forEach(function (g, idx) {
                    if (!g.vertex_count || !g.face_count) return;
                    const color = COLORS[idx % COLORS.length];
                    const r = (color >> 16) & 0xff;
                    const gv = (color >> 8) & 0xff;
                    const b = color & 0xff;
                    const row = document.createElement('div');
                    row.className = 'mv-mat-row';
                    row.innerHTML =
                        '<div class="mv-mat-swatch" style="background:rgb(' + r + ',' + gv + ',' + b + ')"></div>' +
                        '<span class="mv-mat-label">Geoset ' + idx + ' <span style="opacity:.5;font-size:11px">' + (g.vertex_count || 0) + 'v / ' + (g.face_count || 0) + 'f' + (g.material_id !== undefined ? ' mat=' + g.material_id : '') + '</span></span>' +
                        '<span class="mv-mat-eye">\ud83d\udc41</span>';
                    row.addEventListener('click', function () {
                        // Toggle all layer meshes for this geoset
                        let firstMesh = null;
                        meshGroup.children.forEach(function (m) {
                            if (m.userData.geoIndex === idx) {
                                if (!firstMesh) firstMesh = m;
                            }
                        });
                        if (!firstMesh) return;
                        const vis = !firstMesh.visible;
                        meshGroup.children.forEach(function (m) {
                            if (m.userData.geoIndex === idx) m.visible = vis;
                        });
                        wireframeGroup.children.forEach(function (w) {
                            if (w.userData.geoIndex === idx) w.visible = vis && wireOn;
                        });
                        row.classList.toggle('mv-hidden', !vis);
                    });
                    geosetList.appendChild(row);
                });
            }

            // Populate materials panel
            if (materialList) {
                materialList.innerHTML = '';
                materials.forEach(function (mat, i) {
                    let item = document.createElement('div');
                    item.className = 'mv-mat-item';

                    let header = document.createElement('div');
                    header.className = 'mv-mat-item-header';

                    let headerLabel = document.createElement('span');
                    headerLabel.className = 'mv-mat-header-label';
                    let headerText = 'Material #' + i;
                    if (mat.priority_plane) headerText += ' (plane: ' + mat.priority_plane + ')';
                    if (mat.flags) headerText += ' [0x' + mat.flags.toString(16) + ']';
                    headerLabel.textContent = headerText;
                    header.appendChild(headerLabel);

                    let eyeBtn = document.createElement('span');
                    eyeBtn.className = 'mv-mat-eye-btn';
                    eyeBtn.textContent = '\ud83d\udc41';
                    eyeBtn.title = 'Toggle material visibility';
                    eyeBtn.addEventListener('click', function (e) {
                        e.stopPropagation();
                        let isHidden = item.classList.contains('mv-hidden');
                        let vis = isHidden; // if hidden → make visible
                        item.classList.toggle('mv-hidden', !vis);
                        meshGroup.children.forEach(function (m) {
                            if (m.userData.materialId === i) {
                                m.visible = vis;
                            }
                        });
                        // Sync wireframes
                        wireframeGroup.children.forEach(function (w) {
                            let geoIdx = w.userData.geoIndex;
                            let anyVisible = meshGroup.children.some(function (mm) { return mm.userData.geoIndex === geoIdx && mm.visible; });
                            w.visible = wireOn && anyVisible;
                        });
                        // Sync geoset panel rows
                        if (geosetList) {
                            let rows = geosetList.querySelectorAll('.mv-mat-row');
                            // geoset panel rows are indexed by geoset index
                            let geosetIndices = new Set();
                            meshGroup.children.forEach(function (m) {
                                if (m.userData.materialId === i) geosetIndices.add(m.userData.geoIndex);
                            });
                            rows.forEach(function (row, ri) {
                                if (geosetIndices.has(ri)) {
                                    row.classList.toggle('mv-hidden', !vis);
                                }
                            });
                        }
                    });
                    header.appendChild(eyeBtn);
                    item.appendChild(header);

                    let layers = mat.layers || [];
                    layers.forEach(function (layer, li) {
                        let layerDiv = document.createElement('div');
                        layerDiv.className = 'mv-mat-layer';

                        let fmName = FILTER_MODE_NAMES[layer.filter_mode] || 'Unknown(' + layer.filter_mode + ')';
                        let tex = textures[layer.texture_id];

                        let layerHtml =
                            '<div class="mv-mat-layer-row"><span class="mv-mat-layer-label">Layer #' + li + '</span></div>' +
                            '<div class="mv-mat-layer-row"><span class="mv-mat-layer-label">Filter:</span> <span>' + fmName + '</span></div>' +
                            '<div class="mv-mat-layer-row"><span class="mv-mat-layer-label">Shading:</span> <span>' + decodeShadingFlags(layer.shading_flags) + '</span></div>' +
                            '<div class="mv-mat-layer-row"><span class="mv-mat-layer-label">Texture:</span> <span title="' + (tex && tex.file_name ? tex.file_name.replace(/"/g, '&quot;') : '') + '">#' + layer.texture_id;

                        if (tex && tex.file_name) {
                            layerHtml += ' — ' + tex.file_name.replace(/\\\\/g, '/');
                        }
                        layerHtml += '</span></div>';
                        layerHtml += '<div class="mv-mat-layer-row"><span class="mv-mat-layer-label">Alpha:</span> <span>' + (layer.alpha !== undefined ? layer.alpha.toFixed(2) : '1.00') + '</span></div>';
                        layerDiv.innerHTML = layerHtml;

                        if (tex && tex.file_name && !tex.replaceable_id && bs) {
                            let thumbUrl = textureUrl(bs, archivePath, tex.file_name);
                            if (thumbUrl) {
                                let thumb = document.createElement('img');
                                thumb.className = 'mv-mat-thumb';
                                thumb.src = thumbUrl;
                                thumb.alt = tex.file_name;
                                thumb.setAttribute('data-mv-tex-index', layer.texture_id);
                                thumb.onerror = function () {
                                    thumb.style.display = 'none';
                                    let ph = document.createElement('div');
                                    ph.className = 'mv-mat-thumb-placeholder';
                                    ph.textContent = 'Texture not found';
                                    thumb.parentNode.replaceChild(ph, thumb);
                                };
                                layerDiv.appendChild(thumb);
                            }
                        } else if (tex && tex.replaceable_id && replaceableTextures && replaceableTextures[tex.replaceable_id] && bs) {
                            let replPath = replaceableTextures[tex.replaceable_id];
                            let thumbUrl = textureUrl(bs, archivePath, replPath);
                            if (thumbUrl) {
                                let thumb = document.createElement('img');
                                thumb.className = 'mv-mat-thumb';
                                thumb.src = thumbUrl;
                                thumb.alt = replPath;
                                thumb.setAttribute('data-mv-tex-index', layer.texture_id);
                                thumb.onerror = function () {
                                    thumb.style.display = 'none';
                                    let ph = document.createElement('div');
                                    ph.className = 'mv-mat-thumb-placeholder';
                                    ph.textContent = 'Texture not found';
                                    thumb.parentNode.replaceChild(ph, thumb);
                                };
                                layerDiv.appendChild(thumb);
                            }
                        } else if (tex && tex.replaceable_id) {
                            let ph = document.createElement('div');
                            ph.className = 'mv-mat-thumb-placeholder';
                            if (tex.replaceable_id === 1) {
                                ph.textContent = '\ud83c\udfa8 Team Color';
                            } else if (tex.replaceable_id === 2) {
                                ph.textContent = '\u2728 Team Glow (ID 2)';
                            } else {
                                ph.textContent = 'Replaceable (ID ' + tex.replaceable_id + ')';
                            }
                            layerDiv.appendChild(ph);
                        }

                        item.appendChild(layerDiv);
                    });

                    materialList.appendChild(item);
                });

                if (materials.length === 0) {
                    materialList.innerHTML = '<div style="padding:8px;opacity:.5">No materials</div>';
                }
            }

            // Build skeleton
            let skelNodes = [];
            bones.forEach(function (b) { skelNodes.push({name: b.name, objectId: b.object_id, parentId: b.parent_id, flags: b.flags, type: 'bone'}); });
            helpers.forEach(function (h) { skelNodes.push({name: h.name, objectId: h.object_id, parentId: h.parent_id, flags: h.flags, type: 'helper'}); });
            attachments.forEach(function (a) { skelNodes.push({name: a.name, objectId: a.object_id, parentId: a.parent_id, flags: a.flags, type: 'attachment'}); });

            let pivotMap = {};
            pp.forEach(function (p, i) { pivotMap[i] = p; });

            let boneLineVerts = [];
            skelNodes.forEach(function (node) {
                if (node.parentId === 0xFFFFFFFF || node.parentId === 4294967295) return;
                let childPivot = pivotMap[node.objectId];
                let parentPivot = pivotMap[node.parentId];
                if (!childPivot || !parentPivot) return;
                boneLineVerts.push(parentPivot[0], parentPivot[1], parentPivot[2]);
                boneLineVerts.push(childPivot[0], childPivot[1], childPivot[2]);
            });

            if (boneLineVerts.length > 0) {
                let skelGeom = new THREE.BufferGeometry();
                skelGeom.setAttribute('position', new THREE.Float32BufferAttribute(boneLineVerts, 3));
                let skelMat = new THREE.LineBasicMaterial({color: 0xffff00, linewidth: 2, transparent: true, opacity: 0.8});
                let skelLines = new THREE.LineSegments(skelGeom, skelMat);
                skelLines.visible = skeletonOn;
                skelLines.userData.skelNodes = skelNodes;
                skeletonGroup.add(skelLines);
            }

            let sphereGeom = new THREE.SphereGeometry(1.5, 8, 6);
            skelNodes.forEach(function (node) {
                let pivot = pivotMap[node.objectId];
                if (!pivot) return;
                let col = node.type === 'bone' ? 0x00ff88 : (node.type === 'attachment' ? 0xff8844 : 0x4488ff);
                let sMat = new THREE.MeshBasicMaterial({color: col, transparent: true, opacity: 0.8});
                let sphere = new THREE.Mesh(sphereGeom, sMat);
                sphere.position.set(pivot[0], pivot[1], pivot[2]);
                sphere.visible = skeletonOn;
                sphere.userData.objectId = node.objectId;
                skeletonGroup.add(sphere);
            });

            // Populate bones panel
            if (bonesList) {
                bonesList.innerHTML = '';
                if (skelNodes.length === 0) {
                    bonesList.innerHTML = '<div style="padding:8px;opacity:.5">No bones</div>';
                } else {
                    function addBoneSection(nodes, label, nodeType) {
                        if (nodes.length === 0) return;
                        let sectionHeader = document.createElement('div');
                        sectionHeader.className = 'mv-mat-item-header';
                        sectionHeader.textContent = label + ' (' + nodes.length + ')';
                        bonesList.appendChild(sectionHeader);

                        nodes.forEach(function (node) {
                            let row = document.createElement('div');
                            row.className = 'mv-mat-row';
                            let parentStr = (node.parentId === 0xFFFFFFFF || node.parentId === 4294967295) ? 'root' : '#' + node.parentId;
                            let colorDot = nodeType === 'bone' ? '#00ff88' : '#4488ff';
                            row.innerHTML =
                                '<div class="mv-mat-swatch" style="background:' + colorDot + '"></div>' +
                                '<span class="mv-mat-label">' + (node.name || '(unnamed)') + ' <span style="opacity:.5;font-size:11px">ID:' + node.objectId + ' → ' + parentStr + '</span></span>';
                            bonesList.appendChild(row);
                        });
                    }

                    addBoneSection(bones.map(function (b) { return {name: b.name, objectId: b.object_id, parentId: b.parent_id, flags: b.flags}; }), '\ud83e\uddb4 Bones', 'bone');
                    addBoneSection(helpers.map(function (h) { return {name: h.name, objectId: h.object_id, parentId: h.parent_id, flags: h.flags}; }), '\ud83d\udd27 Helpers', 'helper');
                    addBoneSection(attachments.map(function (a) { return {name: a.name, objectId: a.object_id, parentId: a.parent_id, flags: a.flags}; }), '\ud83d\udcce Attachments', 'attachment');
                }
            }

            // Build animation panel
            buildAnimUI(sequences);

            // Info bar
            if (infoEl) {
                let infoText = geosets.length + ' geoset(s) | ' + totalVerts + ' verts | ' + totalFaces + ' faces';
                if (bones.length > 0 || helpers.length > 0) {
                    infoText += ' | ' + bones.length + ' bone(s)';
                    if (helpers.length > 0) infoText += ', ' + helpers.length + ' helper(s)';
                    if (attachments.length > 0) infoText += ', ' + attachments.length + ' attach(s)';
                }
                if (sequences.length > 0) {
                    infoText += ' | ' + sequences.length + ' anim(s)';
                }
                infoEl.textContent = infoText;
            }

            // Auto-fit camera
            const box = new THREE.Box3();
            meshGroup.children.forEach(function (m) {
                m.geometry.computeBoundingBox();
                const cb = m.geometry.boundingBox.clone();
                cb.applyMatrix4(m.matrixWorld);
                box.union(cb);
            });

            const tempGroup = new THREE.Group();
            tempGroup.rotation.x = -Math.PI / 2;
            tempGroup.updateMatrixWorld(true);

            const center = new THREE.Vector3();
            box.getCenter(center);
            center.applyMatrix4(tempGroup.matrixWorld);

            const size = new THREE.Vector3();
            box.getSize(size);
            maxDim = Math.max(size.x, size.y, size.z) || 100;
            ctrl.maxDist = maxDim;

            const dist = maxDim * 1.5;
            ctrl.target.copy(center);
            defaultCamTarget = center.clone();

            const d2 = new THREE.Vector3().set(dist * 0.7, dist * 0.5, dist * 0.7);
            camera.position.copy(center).add(d2);
            camera.lookAt(center);

            camera.near = maxDim * 0.001;
            camera.far = maxDim * 20;
            camera.updateProjectionMatrix();

            win.show();
            onResize();
            if (!animating) {
                animating = true;
                animate();
            }
        }

        function showUnsupported(msg) {
            meshGroup.clear();
            wireframeGroup.clear();
            skeletonGroup.clear();
            if (geosetList) geosetList.innerHTML = '';
            if (materialList) materialList.innerHTML = '';
            if (bonesList) bonesList.innerHTML = '';
            if (animList) animList.innerHTML = '';
            if (nameEl) nameEl.textContent = msg.name || 'Model';
            if (infoEl) infoEl.textContent = '\u26a0 ' + (msg.reason || 'Unsupported format');
            win.show();
        }

        return {load, showUnsupported};
    }

    return { init };
})();

