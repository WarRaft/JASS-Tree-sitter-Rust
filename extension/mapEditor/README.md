# Map Editor

Built-in Warcraft III map editor. Opens binary map files (`.w3e`, `.w3i`, `.doo`, etc.) in an internal VS Code window via the Custom Editor API (`webviewPanel`).

## What counts as a map

- **`.w3x` / `.w3m` folder** — an extracted map on disk. The editor locates the map root by walking up the file path until it finds a directory whose name ends with `.w3x` or `.w3m`.
- **`.w3x` / `.w3m` archive** — a packed map (MPQ archive). Its contents are read through the LSP server and the HTTP binary server without extracting to disk.

Both variants are handled the same way: the editor loads terrain, units, doodads, and map info, then displays everything in a single webview.

## How binary files are opened

1. The user opens a file (`.w3e`, `.w3i`, `.doo`) or an archive (`.w3x`, `.w3m`).
2. VS Code calls `resolveCustomEditor` → `resolveW3eEditor()` from `index.js`.
3. The file type is determined by extension; the access method (file / folder / MPQ archive) is detected automatically.
4. Data is requested from the LSP server via `w3e/render`, `w3i/render`, and `doo/render` requests.
5. For archives the file list comes from `mpq/info`; binary data is served by the HTTP binary server (`/w3e/file`, `/w3e/snapshot`).
6. An HTML page with a Three.js canvas is rendered inside the `webviewPanel`, displaying terrain, placed objects, map information, and a file tree.

## Structure

| File | Description |
|---|---|
| `index.js` | Entry point: data loading, webview creation, message handling |
| `resolveBlpEditor.js` | Standalone BLP image viewer (used when opening `.blp` files outside the map editor) |
| `resolveDooEditor.js` | Standalone DOO file viewer (units/doodads/cliffs table) |
| `mapRoot.js` | Map root discovery (`.w3x`/`.w3m` folder) and binary file scanning |
| `render.js` | HTML generation for the webview |
| `panels.js` | Panels: Map Info, Header, Game Path, Files, W3i, DOO |
| `styles.js` | Webview CSS styles |
| `terrain.js` | Terrain mesh construction for Three.js |
| `utils.js` | Utilities: HTML escaping, formatting |
| `snapshot-types.js` | Data types for the game snapshot |
| `webview/` | Client-side webview scripts (UI, 3D rendering, orbit camera, models) |
