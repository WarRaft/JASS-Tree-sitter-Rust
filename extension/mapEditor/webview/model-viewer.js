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

        var ctrl = window._W3E_ORBIT.makeOrbitControls(camera, canvas, maxDim, {skipGuards: true});

        // Toolbar buttons
        const wireBtn = document.getElementById('mvWireBtn');
        const axesBtn = document.getElementById('mvAxesBtn');
        const gridBtn = document.getElementById('mvGridBtn');
        const resetBtn = document.getElementById('mvResetCamera');
        const geosetBtn = document.getElementById('mvGeosetBtn');
        const geosetsPanel = document.getElementById('mvGeosetsPanel');
        const geosetList = document.getElementById('mvGeosetList');
        const materialBtn = document.getElementById('mvMaterialBtn');
        const materialsPanel = document.getElementById('mvMaterialsPanel');
        const materialList = document.getElementById('mvMaterialList');
        const bonesBtn = document.getElementById('mvBonesBtn');
        const bonesPanel = document.getElementById('mvBonesPanel');
        const bonesList = document.getElementById('mvBonesList');
        const skeletonBtn = document.getElementById('mvSkeletonBtn');

        let wireOn = false, axesOn = true, gridOn = true, skeletonOn = false;

        function toggleSbBtn(btn, on) {
            if (on) btn.classList.add('active');
            else btn.classList.remove('active');
        }

        if (wireBtn) wireBtn.addEventListener('click', function () {
            wireOn = !wireOn;
            toggleSbBtn(wireBtn, wireOn);
            wireframeGroup.children.forEach(function (m, i) {
                var mainMesh = meshGroup.children[i];
                m.visible = wireOn && (!mainMesh || mainMesh.visible);
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

        // Panel toggles
        if (geosetBtn && geosetsPanel) {
            geosetBtn.addEventListener('click', function () {
                const show = geosetsPanel.hidden;
                geosetsPanel.hidden = !show;
                toggleSbBtn(geosetBtn, show);
                if (show && materialsPanel && !materialsPanel.hidden) {
                    materialsPanel.hidden = true; toggleSbBtn(materialBtn, false);
                }
                if (show && bonesPanel && !bonesPanel.hidden) {
                    bonesPanel.hidden = true; toggleSbBtn(bonesBtn, false);
                }
            });
        }
        if (materialBtn && materialsPanel) {
            materialBtn.addEventListener('click', function () {
                const show = materialsPanel.hidden;
                materialsPanel.hidden = !show;
                toggleSbBtn(materialBtn, show);
                if (show && geosetsPanel && !geosetsPanel.hidden) {
                    geosetsPanel.hidden = true; toggleSbBtn(geosetBtn, false);
                }
                if (show && bonesPanel && !bonesPanel.hidden) {
                    bonesPanel.hidden = true; toggleSbBtn(bonesBtn, false);
                }
            });
        }
        if (bonesBtn && bonesPanel) {
            bonesBtn.addEventListener('click', function () {
                const show = bonesPanel.hidden;
                bonesPanel.hidden = !show;
                toggleSbBtn(bonesBtn, show);
                if (show && geosetsPanel && !geosetsPanel.hidden) {
                    geosetsPanel.hidden = true; toggleSbBtn(geosetBtn, false);
                }
                if (show && materialsPanel && !materialsPanel.hidden) {
                    materialsPanel.hidden = true; toggleSbBtn(materialBtn, false);
                }
            });
        }
        if (skeletonBtn) {
            skeletonBtn.addEventListener('click', function () {
                skeletonOn = !skeletonOn;
                toggleSbBtn(skeletonBtn, skeletonOn);
                skeletonGroup.children.forEach(function (c) { c.visible = skeletonOn; });
            });
        }

        // Panel resize handles
        document.querySelectorAll('.mv-panel-resize-handle').forEach(function (handle) {
            handle.addEventListener('mousedown', function (e) {
                e.preventDefault();
                e.stopPropagation();
                var panel = handle.parentElement;
                if (!panel) return;
                var startX = e.clientX;
                var startW = panel.offsetWidth;
                handle.classList.add('active');
                function onMove(ev) {
                    ev.preventDefault();
                    var delta = startX - ev.clientX;
                    var newW = Math.max(120, Math.min(panel.parentElement.clientWidth * 0.8, startW + delta));
                    panel.style.width = newW + 'px';
                }
                function onUp() {
                    handle.classList.remove('active');
                    document.removeEventListener('mousemove', onMove);
                    document.removeEventListener('mouseup', onUp);
                }
                document.addEventListener('mousemove', onMove);
                document.addEventListener('mouseup', onUp);
            });
        });

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
        function animate() {
            if (!animating) return;
            requestAnimationFrame(animate);
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

        function b64ToFloat32(b64) {
            if (!b64) return new Float32Array(0);
            const bin = atob(b64);
            const buf = new ArrayBuffer(bin.length);
            const u8 = new Uint8Array(buf);
            for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i);
            return new Float32Array(buf);
        }

        function b64ToUint16(b64) {
            if (!b64) return new Uint16Array(0);
            const bin = atob(b64);
            const buf = new ArrayBuffer(bin.length);
            const u8 = new Uint8Array(buf);
            for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i);
            return new Uint16Array(buf);
        }

        var FILTER_MODE_NAMES = [
            'None', 'Transparent', 'Blend', 'Additive',
            'AddAlpha', 'Modulate', 'Modulate2x'
        ];

        var SHADING_FLAG_BITS = [
            {bit: 0x01, name: 'Unshaded'},
            {bit: 0x02, name: 'SphereEnvMap'},
            {bit: 0x10, name: 'TwoSided'},
            {bit: 0x20, name: 'Unfogged'},
            {bit: 0x40, name: 'NoDepthTest'},
            {bit: 0x80, name: 'NoDepthSet'},
        ];

        function decodeShadingFlags(flags) {
            var names = [];
            for (var i = 0; i < SHADING_FLAG_BITS.length; i++) {
                if (flags & SHADING_FLAG_BITS[i].bit) names.push(SHADING_FLAG_BITS[i].name);
            }
            return names.length > 0 ? names.join(', ') : 'None';
        }


        function textureUrl(bs, archivePath, texPath) {
            if (!bs || !texPath) return null;
            var params = new URLSearchParams({
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
            const pivotPoints = msg.pivot_points || [];
            const bs = msg.binaryServer || window.__W3E_DATA__.binaryServer || null;
            const archivePath = msg.archivePath || window.__W3E_DATA__.archivePath || null;
            const replaceableTextures = msg.replaceableTextures || null;

            if (geosets.length === 0) {
                if (infoEl) infoEl.textContent = 'No geosets';
                win.show();
                return;
            }

            var loadedTextures = new Array(textures.length).fill(null);
            var textureLoader = new THREE.TextureLoader();
            textureLoader.crossOrigin = 'anonymous';

            function getTextureForMaterial(materialId) {
                if (materialId < materials.length) {
                    var mat = materials[materialId];
                    var layers = mat.layers || [];
                    if (layers.length > 0) {
                        var texId = layers[0].texture_id;
                        if (texId < loadedTextures.length && loadedTextures[texId]) {
                            return {texture: loadedTextures[texId], layer: layers[0], texIndex: texId};
                        }
                    }
                }
                return null;
            }

            function getLayerForMaterial(materialId) {
                if (materialId < materials.length) {
                    var mat = materials[materialId];
                    var layers = mat.layers || [];
                    if (layers.length > 0) return layers[0];
                }
                return null;
            }

            let totalVerts = 0, totalFaces = 0;

            geosets.forEach(function (g, idx) {
                if (!g.vertex_count || !g.face_count) return;
                const vertices = b64ToFloat32(g.vertices);
                const normals = b64ToFloat32(g.normals);
                const faces = b64ToUint16(g.faces);
                const uvs = b64ToFloat32(g.uvs);

                totalVerts += g.vertex_count;
                totalFaces += g.face_count;

                const geometry = new THREE.BufferGeometry();
                geometry.setAttribute('position', new THREE.BufferAttribute(vertices, 3));
                if (normals.length > 0) geometry.setAttribute('normal', new THREE.BufferAttribute(normals, 3));
                if (uvs.length > 0) geometry.setAttribute('uv', new THREE.BufferAttribute(uvs, 2));
                geometry.setIndex(new THREE.BufferAttribute(faces, 1));
                if (normals.length === 0) geometry.computeVertexNormals();

                const color = COLORS[idx % COLORS.length];
                var texInfo = getTextureForMaterial(g.material_id);
                var layer = getLayerForMaterial(g.material_id);
                var sf = layer ? layer.shading_flags : 0;
                var fm = layer ? layer.filter_mode : 0;

                var matOpts = { flatShading: false };

                // TwoSided (0x10) → DoubleSide, otherwise FrontSide
                matOpts.side = (sf & 0x10) ? THREE.DoubleSide : THREE.DoubleSide;

                // NoDepthTest (0x40)
                if (sf & 0x40) matOpts.depthTest = false;

                // NoDepthSet (0x80)
                if (sf & 0x80) matOpts.depthWrite = false;

                if (texInfo) {
                    matOpts.map = texInfo.texture;
                }

                // Blending modes matching MdlVis Real3D.pas
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

                // Unshaded (0x01) → use MeshBasicMaterial (no lighting)
                var material;
                if (sf & 0x01) {
                    material = new THREE.MeshBasicMaterial(matOpts);
                } else {
                    material = new THREE.MeshPhongMaterial(matOpts);
                }
                material.userData = {hasTexture: !!texInfo, fallbackColor: color, materialId: g.material_id};
                const mesh = new THREE.Mesh(geometry, material);
                mesh.userData.geoIndex = idx;
                mesh.userData.materialId = g.material_id;
                meshGroup.add(mesh);

                const wireMat = new THREE.MeshBasicMaterial({
                    color: 0xffffff, wireframe: true, transparent: true, opacity: 0.15,
                });
                const wireMesh = new THREE.Mesh(geometry, wireMat);
                wireMesh.visible = wireOn;
                wireframeGroup.add(wireMesh);
            });

            // Load textures
            if (bs) {
                textures.forEach(function (tex, i) {
                    if (!tex) return;
                    var actualPath = null;
                    if (tex.replaceable_id && replaceableTextures) {
                        if (replaceableTextures._cliffTex0 !== undefined) {
                            actualPath = (tex.replaceable_id % 2 === 0)
                                ? replaceableTextures._cliffTex0
                                : replaceableTextures._cliffTex1;
                        } else if (replaceableTextures[tex.replaceable_id]) {
                            actualPath = replaceableTextures[tex.replaceable_id];
                        }
                    } else if (tex.file_name && !tex.replaceable_id) {
                        actualPath = tex.file_name;
                    }
                    if (!actualPath) return;
                    var url = textureUrl(bs, archivePath, actualPath);
                    if (!url) return;

                    var threeTex = textureLoader.load(url, function () {
                        meshGroup.children.forEach(function (m) {
                            var matId = m.userData.materialId;
                            var info = getTextureForMaterial(matId);
                            if (info && info.texIndex === i) {
                                m.material.map = threeTex;
                                m.material.color.set(0xffffff);
                                m.material.needsUpdate = true;
                            }
                        });
                        var imgs = document.querySelectorAll('[data-mv-tex-index="' + i + '"]');
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
                        const mesh = meshGroup.children[idx];
                        const wire = wireframeGroup.children[idx];
                        if (!mesh) return;
                        const vis = !mesh.visible;
                        mesh.visible = vis;
                        if (wire) wire.visible = vis && wireOn;
                        row.classList.toggle('mv-hidden', !vis);
                    });
                    geosetList.appendChild(row);
                });
            }

            // Populate materials panel
            if (materialList) {
                materialList.innerHTML = '';
                materials.forEach(function (mat, i) {
                    var item = document.createElement('div');
                    item.className = 'mv-mat-item';

                    var header = document.createElement('div');
                    header.className = 'mv-mat-item-header';
                    var headerText = 'Material #' + i;
                    if (mat.priority_plane) headerText += ' (plane: ' + mat.priority_plane + ')';
                    if (mat.flags) headerText += ' [0x' + mat.flags.toString(16) + ']';
                    header.textContent = headerText;
                    item.appendChild(header);

                    var layers = mat.layers || [];
                    layers.forEach(function (layer, li) {
                        var layerDiv = document.createElement('div');
                        layerDiv.className = 'mv-mat-layer';

                        var fmName = FILTER_MODE_NAMES[layer.filter_mode] || 'Unknown(' + layer.filter_mode + ')';
                        var tex = textures[layer.texture_id];

                        var layerHtml =
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
                            var thumbUrl = textureUrl(bs, archivePath, tex.file_name);
                            if (thumbUrl) {
                                var thumb = document.createElement('img');
                                thumb.className = 'mv-mat-thumb';
                                thumb.src = thumbUrl;
                                thumb.alt = tex.file_name;
                                thumb.setAttribute('data-mv-tex-index', layer.texture_id);
                                thumb.onerror = function () {
                                    thumb.style.display = 'none';
                                    var ph = document.createElement('div');
                                    ph.className = 'mv-mat-thumb-placeholder';
                                    ph.textContent = 'Texture not found';
                                    thumb.parentNode.replaceChild(ph, thumb);
                                };
                                layerDiv.appendChild(thumb);
                            }
                        } else if (tex && tex.replaceable_id && replaceableTextures && replaceableTextures[tex.replaceable_id] && bs) {
                            var replPath = replaceableTextures[tex.replaceable_id];
                            var thumbUrl = textureUrl(bs, archivePath, replPath);
                            if (thumbUrl) {
                                var thumb = document.createElement('img');
                                thumb.className = 'mv-mat-thumb';
                                thumb.src = thumbUrl;
                                thumb.alt = replPath;
                                thumb.setAttribute('data-mv-tex-index', layer.texture_id);
                                thumb.onerror = function () {
                                    thumb.style.display = 'none';
                                    var ph = document.createElement('div');
                                    ph.className = 'mv-mat-thumb-placeholder';
                                    ph.textContent = 'Texture not found';
                                    thumb.parentNode.replaceChild(ph, thumb);
                                };
                                layerDiv.appendChild(thumb);
                            }
                        } else if (tex && tex.replaceable_id) {
                            var ph = document.createElement('div');
                            ph.className = 'mv-mat-thumb-placeholder';
                            ph.textContent = 'Replaceable (ID ' + tex.replaceable_id + ')';
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
            var allNodes = [];
            bones.forEach(function (b) { allNodes.push({name: b.name, objectId: b.object_id, parentId: b.parent_id, flags: b.flags, type: 'bone'}); });
            helpers.forEach(function (h) { allNodes.push({name: h.name, objectId: h.object_id, parentId: h.parent_id, flags: h.flags, type: 'helper'}); });

            var pivotMap = {};
            pivotPoints.forEach(function (p, i) { pivotMap[i] = p; });

            var boneLineVerts = [];
            allNodes.forEach(function (node) {
                if (node.parentId === 0xFFFFFFFF || node.parentId === 4294967295) return;
                var childPivot = pivotMap[node.objectId];
                var parentPivot = pivotMap[node.parentId];
                if (!childPivot || !parentPivot) return;
                boneLineVerts.push(parentPivot[0], parentPivot[1], parentPivot[2]);
                boneLineVerts.push(childPivot[0], childPivot[1], childPivot[2]);
            });

            if (boneLineVerts.length > 0) {
                var skelGeom = new THREE.BufferGeometry();
                skelGeom.setAttribute('position', new THREE.Float32BufferAttribute(boneLineVerts, 3));
                var skelMat = new THREE.LineBasicMaterial({color: 0xffff00, linewidth: 2, transparent: true, opacity: 0.8});
                var skelLines = new THREE.LineSegments(skelGeom, skelMat);
                skelLines.visible = skeletonOn;
                skeletonGroup.add(skelLines);
            }

            var sphereGeom = new THREE.SphereGeometry(1.5, 8, 6);
            allNodes.forEach(function (node) {
                var pivot = pivotMap[node.objectId];
                if (!pivot) return;
                var col = node.type === 'bone' ? 0x00ff88 : 0x4488ff;
                var sMat = new THREE.MeshBasicMaterial({color: col, transparent: true, opacity: 0.8});
                var sphere = new THREE.Mesh(sphereGeom, sMat);
                sphere.position.set(pivot[0], pivot[1], pivot[2]);
                sphere.visible = skeletonOn;
                skeletonGroup.add(sphere);
            });

            // Populate bones panel
            if (bonesList) {
                bonesList.innerHTML = '';
                if (allNodes.length === 0) {
                    bonesList.innerHTML = '<div style="padding:8px;opacity:.5">No bones</div>';
                } else {
                    function addBoneSection(nodes, label, nodeType) {
                        if (nodes.length === 0) return;
                        var sectionHeader = document.createElement('div');
                        sectionHeader.className = 'mv-mat-item-header';
                        sectionHeader.textContent = label + ' (' + nodes.length + ')';
                        bonesList.appendChild(sectionHeader);

                        nodes.forEach(function (node) {
                            var row = document.createElement('div');
                            row.className = 'mv-mat-row';
                            var parentStr = (node.parentId === 0xFFFFFFFF || node.parentId === 4294967295) ? 'root' : '#' + node.parentId;
                            var colorDot = nodeType === 'bone' ? '#00ff88' : '#4488ff';
                            row.innerHTML =
                                '<div class="mv-mat-swatch" style="background:' + colorDot + '"></div>' +
                                '<span class="mv-mat-label">' + (node.name || '(unnamed)') + ' <span style="opacity:.5;font-size:11px">ID:' + node.objectId + ' → ' + parentStr + '</span></span>';
                            bonesList.appendChild(row);
                        });
                    }

                    addBoneSection(bones.map(function (b) { return {name: b.name, objectId: b.object_id, parentId: b.parent_id, flags: b.flags}; }), '\ud83e\uddb4 Bones', 'bone');
                    addBoneSection(helpers.map(function (h) { return {name: h.name, objectId: h.object_id, parentId: h.parent_id, flags: h.flags}; }), '\ud83d\udd27 Helpers', 'helper');
                }
            }

            // Info bar
            if (infoEl) {
                var infoText = geosets.length + ' geoset(s) | ' + totalVerts + ' verts | ' + totalFaces + ' faces';
                if (bones.length > 0 || helpers.length > 0) {
                    infoText += ' | ' + bones.length + ' bone(s)';
                    if (helpers.length > 0) infoText += ', ' + helpers.length + ' helper(s)';
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
            if (nameEl) nameEl.textContent = msg.name || 'Model';
            if (infoEl) infoEl.textContent = '\u26a0 ' + (msg.reason || 'Unsupported format');
            win.show();
        }

        return {load, showUnsupported};
    }

    return { init };
})();

