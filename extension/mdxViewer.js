// noinspection JSUnresolvedReference
(function () {
    'use strict';

    const {MODEL, GEOSETS_META, GEOSETS_B64, MATERIALS, TEXTURES, BINARY_SERVER, ARCHIVE_PATH} = window.MDX_INIT;

    // ── Base64 → TypedArray (zero-copy via ArrayBuffer) ─────
    function b64ToArrayBuffer(b64) {
        const bin = atob(b64);
        const len = bin.length;
        const buf = new ArrayBuffer(len);
        const u8 = new Uint8Array(buf);
        for (let i = 0; i < len; i++) u8[i] = bin.charCodeAt(i);
        return buf;
    }

    function b64ToFloat32(b64) {
        if (!b64) return new Float32Array(0);
        return new Float32Array(b64ToArrayBuffer(b64));
    }

    function b64ToUint16(b64) {
        if (!b64) return new Uint16Array(0);
        return new Uint16Array(b64ToArrayBuffer(b64));
    }

    // ── Info bar ────────────────────────────────────────────
    const infoEl = document.getElementById('model-info');
    infoEl.textContent = 'v' + MODEL.version +
        ' | ' + MODEL.geosetCount + ' geoset(s)' +
        ' | ' + MODEL.totalVertices + ' vertices' +
        ' | ' + MODEL.totalFaces + ' faces' +
        ' | ' + (MODEL.size / 1024).toFixed(1) + ' KB';

    // ── Populate geoset selector ────────────────────────────
    const geoSelect = document.getElementById('geoset-select');
    GEOSETS_META.forEach(function (g, i) {
        const opt = document.createElement('option');
        opt.value = String(i);
        opt.textContent = 'Geoset #' + (i + 1) +
            ' (' + g.vertexCount + ' verts, mat=' + g.materialId + ')';
        geoSelect.appendChild(opt);
    });

    // ── Three.js Scene ──────────────────────────────────────
    const container = document.getElementById('canvas-container');
    const canvas = document.getElementById('viewport');

    const renderer = new THREE.WebGLRenderer({canvas, antialias: true, alpha: false});
    renderer.setPixelRatio(window.devicePixelRatio);
    renderer.setClearColor(0x1e1e1e);

    const scene = new THREE.Scene();

    // Camera
    const camera = new THREE.PerspectiveCamera(45, 1, 0.1, 10000);
    camera.position.set(300, 200, 300);
    camera.lookAt(0, 50, 0);

    // Lights
    scene.add(new THREE.AmbientLight(0x606060));
    const dirLight = new THREE.DirectionalLight(0xffffff, 0.8);
    dirLight.position.set(200, 400, 300);
    scene.add(dirLight);
    const dirLight2 = new THREE.DirectionalLight(0x4488ff, 0.3);
    dirLight2.position.set(-200, 100, -300);
    scene.add(dirLight2);

    // Grid & Axes
    const gridHelper = new THREE.GridHelper(500, 20, 0x444444, 0x333333);
    scene.add(gridHelper);
    const axesHelper = new THREE.AxesHelper(100);
    scene.add(axesHelper);

    // ── Build meshes from server-parsed TypedArrays ─────────
    const GEOSET_COLORS = [
        0x4fc3f7, 0xab47bc, 0x66bb6a, 0xffa726,
        0xef5350, 0x26c6da, 0xd4e157, 0xec407a,
        0x42a5f5, 0x8d6e63, 0x78909c, 0xffca28,
    ];

    // ── Pre-load textures via HTTP server ─────────────────────
    const loadedTextures = new Array(TEXTURES.length).fill(null);
    const textureLoader = new THREE.TextureLoader();

    /** Build texture URL for the HTTP server. */
    function textureUrl(texPath) {
        if (!BINARY_SERVER || !texPath) return null;
        const params = new URLSearchParams({
            token: BINARY_SERVER.token,
            path: texPath,
        });
        if (ARCHIVE_PATH) params.set('archive', ARCHIVE_PATH);
        return 'http://127.0.0.1:' + BINARY_SERVER.port + '/mdx/texture?' + params;
    }

    /** Look up the THREE.Texture for a geoset by its materialId. */
    function getTextureForMaterial(materialId) {
        if (materialId < MATERIALS.length) {
            const mat = MATERIALS[materialId];
            if (mat.layers.length > 0) {
                var texId = mat.layers[0].textureId;
                if (texId < loadedTextures.length && loadedTextures[texId]) {
                    return {texture: loadedTextures[texId], layer: mat.layers[0], texIndex: texId};
                }
            }
        }
        return null;
    }

    const meshGroup = new THREE.Group();
    const wireframeGroup = new THREE.Group();
    const normalsGroup = new THREE.Group();

    // WarCraft III MDX uses Z-up, Three.js uses Y-up
    const rootGroup = new THREE.Group();
    rootGroup.rotation.x = -Math.PI / 2;
    rootGroup.add(meshGroup);
    rootGroup.add(wireframeGroup);
    rootGroup.add(normalsGroup);
    scene.add(rootGroup);

    // Filter mode names
    var FILTER_MODE_NAMES = [
        'None', 'Transparent', 'Blend', 'Additive',
        'AddAlpha', 'Modulate', 'Modulate2x'
    ];

    /** Update thumbnail images after a texture finishes loading. */
    function updateTextureThumbnails(texIndex, imgUrl) {
        var imgs = document.querySelectorAll('[data-tex-index="' + texIndex + '"]');
        imgs.forEach(function (el) {
            if (el.tagName === 'IMG') {
                el.src = imgUrl;
                el.style.display = '';
            } else {
                // Replace loading/placeholder with img
                var img = document.createElement('img');
                img.src = imgUrl;
                img.alt = TEXTURES[texIndex].fileName || 'Texture #' + texIndex;
                img.setAttribute('data-tex-index', texIndex);
                if (el.classList.contains('ml-tex-thumb')) {
                    img.className = 'ml-tex-thumb';
                }
                el.parentNode.replaceChild(img, el);
            }
        });
    }

    /** Async-load a single texture from the server. Updates mesh materials when done. */
    function loadTexture(index) {
        const tex = TEXTURES[index];
        if (!tex || !tex.fileName || tex.replaceableId) return;
        const url = textureUrl(tex.fileName);
        if (!url) return;

        const threeTex = textureLoader.load(url, function () {
            // Texture loaded — update all meshes that reference it
            meshGroup.children.forEach(function (m) {
                var matId = GEOSETS_META[m.userData.geoIndex].materialId;
                var info = getTextureForMaterial(matId);
                if (info && info.texIndex === index) {
                    m.material.map = threeTex;
                    m.material.needsUpdate = true;
                }
            });
            // Update panel thumbnails
            updateTextureThumbnails(index, url);
        });
        threeTex.wrapS = THREE.RepeatWrapping;
        threeTex.wrapT = THREE.RepeatWrapping;
        threeTex.magFilter = THREE.LinearFilter;
        threeTex.minFilter = THREE.LinearMipmapLinearFilter;
        loadedTextures[index] = threeTex;
    }

    GEOSETS_B64.forEach(function (b64, idx) {
        const meta = GEOSETS_META[idx];
        if (meta.vertexCount === 0 || meta.faceCount === 0) return;

        // Decode base64 → TypedArray (server already parsed & flipped UVs)
        const vertices = b64ToFloat32(b64.vertices);
        const normals = b64ToFloat32(b64.normals);
        const faces = b64ToUint16(b64.faces);
        const uvs = b64ToFloat32(b64.uvs);

        const geometry = new THREE.BufferGeometry();
        geometry.setAttribute('position', new THREE.BufferAttribute(vertices, 3));
        if (normals.length > 0) {
            geometry.setAttribute('normal', new THREE.BufferAttribute(normals, 3));
        }
        if (uvs.length > 0) {
            geometry.setAttribute('uv', new THREE.BufferAttribute(uvs, 2));
        }
        geometry.setIndex(new THREE.BufferAttribute(faces, 1));
        if (normals.length === 0) geometry.computeVertexNormals();

        const color = GEOSET_COLORS[idx % GEOSET_COLORS.length];
        const texInfo = getTextureForMaterial(meta.materialId);

        // Solid mesh — textured if available, colored otherwise
        const matOpts = {
            side: THREE.DoubleSide,
            flatShading: false,
        };
        if (texInfo) {
            matOpts.map = texInfo.texture;
            // Handle filter_mode for blending / transparency
            var fm = texInfo.layer.filterMode;
            if (fm === 1) {
                // Transparent
                matOpts.transparent = true;
                matOpts.alphaTest = 0.5;
            } else if (fm === 2 || fm === 3) {
                // Blend / Additive
                matOpts.transparent = true;
                matOpts.blending = fm === 3 ? THREE.AdditiveBlending : THREE.NormalBlending;
                matOpts.depthWrite = false;
            } else {
                matOpts.transparent = false;
            }
            if (texInfo.layer.alpha < 1.0) {
                matOpts.transparent = true;
                matOpts.opacity = texInfo.layer.alpha;
            }
        } else {
            matOpts.color = color;
            matOpts.transparent = true;
            matOpts.opacity = 0.95;
        }
        const material = new THREE.MeshPhongMaterial(matOpts);
        material.userData = {hasTexture: !!texInfo, fallbackColor: color};
        const mesh = new THREE.Mesh(geometry, material);
        mesh.userData.geoIndex = idx;
        meshGroup.add(mesh);

        // Wireframe overlay
        const wireMat = new THREE.MeshBasicMaterial({
            color: 0xffffff,
            wireframe: true,
            transparent: true,
            opacity: 0.15,
        });
        const wireMesh = new THREE.Mesh(geometry, wireMat);
        wireMesh.userData.geoIndex = idx;
        wireMesh.visible = false;
        wireframeGroup.add(wireMesh);

        // Vertex normals helper — pre-computed on the server as a TypedArray
        const normalLineVerts = b64ToFloat32(b64.normalLines);
        if (normalLineVerts.length > 0) {
            const lineGeom = new THREE.BufferGeometry();
            lineGeom.setAttribute('position', new THREE.BufferAttribute(normalLineVerts, 3));
            const lineMat = new THREE.LineBasicMaterial({color: 0x00ff00, transparent: true, opacity: 0.4});
            const lines = new THREE.LineSegments(lineGeom, lineMat);
            lines.userData.geoIndex = idx;
            lines.visible = false;
            normalsGroup.add(lines);
        }
    });

    // ── Start loading textures now that meshes exist ──────────
    if (BINARY_SERVER) {
        TEXTURES.forEach(function (_, i) { loadTexture(i); });
    }

    // ── Auto-fit camera ─────────────────────────────────────
    const box = new THREE.Box3();
    meshGroup.children.forEach(function (m) {
        m.geometry.computeBoundingBox();
        const childBox = m.geometry.boundingBox.clone();
        childBox.applyMatrix4(m.matrixWorld);
        box.union(childBox);
    });

    const tempGroup = new THREE.Group();
    tempGroup.rotation.x = -Math.PI / 2;
    tempGroup.updateMatrixWorld(true);

    const center = new THREE.Vector3();
    box.getCenter(center);
    center.applyMatrix4(tempGroup.matrixWorld);

    const size = new THREE.Vector3();
    box.getSize(size);
    const maxDim = Math.max(size.x, size.y, size.z) || 100;

    const dist = maxDim * 1.5;
    camera.position.set(center.x + dist * 0.7, center.y + dist * 0.5, center.z + dist * 0.7);
    camera.lookAt(center);
    camera.near = maxDim * 0.001;
    camera.far = maxDim * 20;
    camera.updateProjectionMatrix();

    const defaultCamPos = camera.position.clone();
    const defaultCamTarget = center.clone();

    // ── Orbit controls ──────────────────────────────────────
    let isDragging = false;
    let isPanning = false;
    let prevMouse = {x: 0, y: 0};
    const spherical = {phi: Math.PI / 4, theta: Math.PI / 4, radius: dist};
    const panTarget = center.clone();

    function updateCameraFromSpherical() {
        const sp = spherical;
        camera.position.set(
            panTarget.x + sp.radius * Math.sin(sp.phi) * Math.cos(sp.theta),
            panTarget.y + sp.radius * Math.cos(sp.phi),
            panTarget.z + sp.radius * Math.sin(sp.phi) * Math.sin(sp.theta)
        );
        camera.lookAt(panTarget);
    }

    // Initialize spherical from current camera position
    const delta = new THREE.Vector3().subVectors(camera.position, panTarget);
    spherical.radius = delta.length();
    spherical.phi = Math.acos(Math.max(-1, Math.min(1, delta.y / spherical.radius)));
    spherical.theta = Math.atan2(delta.z, delta.x);

    canvas.addEventListener('mousedown', function (e) {
        if (e.button === 0) isDragging = true;
        if (e.button === 1 || e.button === 2) isPanning = true;
        prevMouse = {x: e.clientX, y: e.clientY};
        e.preventDefault();
    });
    canvas.addEventListener('contextmenu', function (e) { e.preventDefault(); });
    window.addEventListener('mouseup', function () { isDragging = false; isPanning = false; });
    window.addEventListener('mousemove', function (e) {
        const dx = e.clientX - prevMouse.x;
        const dy = e.clientY - prevMouse.y;
        prevMouse = {x: e.clientX, y: e.clientY};

        if (isDragging) {
            spherical.theta -= dx * 0.005;
            spherical.phi = Math.max(0.01, Math.min(Math.PI - 0.01, spherical.phi + dy * 0.005));
            updateCameraFromSpherical();
        }
        if (isPanning) {
            const panSpeed = spherical.radius * 0.001;
            const right = new THREE.Vector3();
            const up = new THREE.Vector3();
            camera.getWorldDirection(new THREE.Vector3());
            right.crossVectors(camera.up, new THREE.Vector3().subVectors(panTarget, camera.position)).normalize();
            up.copy(camera.up);
            panTarget.addScaledVector(right, dx * panSpeed);
            panTarget.addScaledVector(up, dy * panSpeed);
            updateCameraFromSpherical();
        }
    });
    canvas.addEventListener('wheel', function (e) {
        e.preventDefault();
        spherical.radius *= 1 + e.deltaY * 0.001;
        spherical.radius = Math.max(maxDim * 0.01, Math.min(maxDim * 50, spherical.radius));
        updateCameraFromSpherical();
    }, {passive: false});

    // ── Toolbar controls ────────────────────────────────────
    document.getElementById('wireframe-toggle').addEventListener('change', function (e) {
        wireframeGroup.children.forEach(function (m) { m.visible = e.target.checked; });
    });
    document.getElementById('normals-toggle').addEventListener('change', function (e) {
        normalsGroup.children.forEach(function (m) { m.visible = e.target.checked; });
    });
    document.getElementById('axes-toggle').addEventListener('change', function (e) {
        axesHelper.visible = e.target.checked;
    });
    document.getElementById('grid-toggle').addEventListener('change', function (e) {
        gridHelper.visible = e.target.checked;
    });
    document.getElementById('reset-camera').addEventListener('click', function () {
        panTarget.copy(defaultCamTarget);
        const d2 = new THREE.Vector3().subVectors(defaultCamPos, panTarget);
        spherical.radius = d2.length();
        spherical.phi = Math.acos(Math.max(-1, Math.min(1, d2.y / spherical.radius)));
        spherical.theta = Math.atan2(d2.z, d2.x);
        updateCameraFromSpherical();
    });
    geoSelect.addEventListener('change', function (e) {
        const val = e.target.value;
        [meshGroup, wireframeGroup, normalsGroup].forEach(function (group) {
            group.children.forEach(function (child) {
                if (val === 'all') {
                    if (group === meshGroup) child.visible = true;
                    if (group === wireframeGroup) child.visible = document.getElementById('wireframe-toggle').checked;
                    if (group === normalsGroup) child.visible = document.getElementById('normals-toggle').checked;
                } else {
                    const show = child.userData.geoIndex === parseInt(val);
                    if (group === meshGroup) child.visible = show;
                    if (group === wireframeGroup) child.visible = show && document.getElementById('wireframe-toggle').checked;
                    if (group === normalsGroup) child.visible = show && document.getElementById('normals-toggle').checked;
                }
            });
        });
    });

    // ── Textured toggle ──────────────────────────────────────
    document.getElementById('textured-toggle').addEventListener('change', function (e) {
        var useTexture = e.target.checked;
        meshGroup.children.forEach(function (m) {
            var mat = m.material;
            if (mat.userData && mat.userData.hasTexture) {
                if (useTexture) {
                    var texInfo = getTextureForMaterial(GEOSETS_META[m.userData.geoIndex].materialId);
                    if (texInfo) mat.map = texInfo.texture;
                    mat.color.set(0xffffff);
                } else {
                    mat.map = null;
                    mat.color.set(mat.userData.fallbackColor);
                    mat.transparent = true;
                    mat.opacity = 0.95;
                }
                mat.needsUpdate = true;
            }
        });
    });

    // ── Textures panel ───────────────────────────────────────
    var texPanel = document.getElementById('textures-panel');
    document.getElementById('textures-btn').addEventListener('click', function () {
        texPanel.classList.toggle('open');
    });
    document.getElementById('textures-close').addEventListener('click', function () {
        texPanel.classList.remove('open');
    });

    // Populate textures panel
    var texBody = document.getElementById('textures-body');
    TEXTURES.forEach(function (tex, i) {
        var item = document.createElement('div');
        item.className = 'tex-item';

        var info = document.createElement('div');
        info.className = 'tex-info';
        var label = '<strong>#' + i + '</strong>';
        if (tex.fileName) label += ' — ' + tex.fileName.replace(/\\\\/g, '/');
        if (tex.replaceableId) label += ' <em>(replaceable ' + tex.replaceableId + ')</em>';
        if (tex.flags) label += ' [flags: ' + tex.flags + ']';
        info.innerHTML = label;
        item.appendChild(info);

        var url = textureUrl(tex.fileName);
        if (url && !tex.replaceableId) {
            var img = document.createElement('img');
            img.src = url;
            img.alt = tex.fileName || 'Texture #' + i;
            img.setAttribute('data-tex-index', i);
            img.onerror = function () {
                var ph = document.createElement('div');
                ph.className = 'tex-placeholder';
                ph.textContent = 'Texture not found';
                img.parentNode.replaceChild(ph, img);
            };
            item.appendChild(img);
        } else {
            var ph = document.createElement('div');
            ph.className = 'tex-placeholder';
            ph.textContent = tex.replaceableId
                ? 'Replaceable Texture (ID ' + tex.replaceableId + ')'
                : BINARY_SERVER ? 'Texture not found' : 'Set game path to load textures';
            item.appendChild(ph);
        }

        texBody.appendChild(item);
    });

    // ── Materials panel ───────────────────────────────────────
    var matPanel = document.getElementById('materials-panel');
    document.getElementById('materials-btn').addEventListener('click', function () {
        matPanel.classList.toggle('open');
    });
    document.getElementById('materials-close').addEventListener('click', function () {
        matPanel.classList.remove('open');
    });

    // Populate materials panel
    var matBody = document.getElementById('materials-body');
    MATERIALS.forEach(function (mat, i) {
        var item = document.createElement('div');
        item.className = 'mat-item';

        var header = document.createElement('div');
        header.className = 'mat-header';
        var headerText = 'Material #' + i;
        if (mat.priorityPlane) headerText += ' (plane: ' + mat.priorityPlane + ')';
        if (mat.flags) headerText += ' [flags: 0x' + mat.flags.toString(16) + ']';
        header.textContent = headerText;
        item.appendChild(header);

        mat.layers.forEach(function (layer, li) {
            var layerDiv = document.createElement('div');
            layerDiv.className = 'mat-layer';

            var fmName = FILTER_MODE_NAMES[layer.filterMode] || 'Unknown(' + layer.filterMode + ')';

            layerDiv.innerHTML =
                '<div class="ml-row"><span class="ml-label">Layer #' + li + '</span></div>' +
                '<div class="ml-row"><span class="ml-label">Filter:</span> <span class="ml-value">' + fmName + '</span></div>' +
                '<div class="ml-row"><span class="ml-label">Shading:</span> <span class="ml-value">0x' + layer.shadingFlags.toString(16) + '</span></div>' +
                '<div class="ml-row"><span class="ml-label">Texture:</span> <span class="ml-value">#' + layer.textureId +
                (TEXTURES[layer.textureId] && TEXTURES[layer.textureId].fileName
                    ? ' — ' + TEXTURES[layer.textureId].fileName.replace(/\\\\/g, '/')
                    : '') +
                '</span></div>' +
                '<div class="ml-row"><span class="ml-label">Alpha:</span> <span class="ml-value">' + layer.alpha.toFixed(2) + '</span></div>';

            // Texture thumbnail
            var texInfo = TEXTURES[layer.textureId];
            if (texInfo && texInfo.fileName && !texInfo.replaceableId) {
                var thumbUrl = textureUrl(texInfo.fileName);
                if (thumbUrl) {
                    var thumb = document.createElement('img');
                    thumb.className = 'ml-tex-thumb';
                    thumb.src = thumbUrl;
                    thumb.alt = texInfo.fileName;
                    thumb.setAttribute('data-tex-index', layer.textureId);
                    thumb.onerror = function () { thumb.style.display = 'none'; };
                    layerDiv.appendChild(thumb);
                }
            }

            item.appendChild(layerDiv);
        });

        matBody.appendChild(item);
    });

    // ── Resize ──────────────────────────────────────────────
    function onResize() {
        const w = container.clientWidth;
        const h = container.clientHeight;
        if (w === 0 || h === 0) return;
        renderer.setSize(w, h);
        camera.aspect = w / h;
        camera.updateProjectionMatrix();
    }

    const resizeObserver = new ResizeObserver(onResize);
    resizeObserver.observe(container);
    onResize();

    // ── Animation loop ──────────────────────────────────────
    function animate() {
        requestAnimationFrame(animate);
        renderer.render(scene, camera);
    }

    animate();
})();






