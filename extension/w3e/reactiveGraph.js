'use strict';

/**
 * Reactive dependency graph (DAG).
 *
 * Nodes form a directed acyclic graph.  When a source node's value
 * is set, all transitive dependents are re-computed in topological
 * order (parents before children) and their subscribers are notified.
 *
 * @example
 *   const g = new ReactiveGraph();
 *
 *   // source nodes — values set externally
 *   g.define('gamePath');
 *
 *   // computed nodes — re-evaluate when any dependency changes
 *   g.define('westrings', ['gamePath'], async ({gamePath}) => fetchWestrings(gamePath));
 *   g.define('terrainSlk', ['gamePath', 'westrings'], async (deps) => fetchTerrain(deps));
 *
 *   // subscribers — react to value changes
 *   g.subscribe('terrainSlk', value => rebuildUI(value));
 *
 *   // trigger cascade
 *   await g.set('gamePath', '/path/to/wc3');
 *   // → westrings recomputes, then terrainSlk recomputes, subscriber fires
 *
 *   // partial update — only dependents of 'westrings' cascade
 *   await g.set('westrings', newWestrings);
 */
class ReactiveGraph {
    constructor() {
        /** @type {Map<string, {deps: string[], compute: Function|null, value: any, subscribers: Function[]}>} */
        this._nodes = new Map();
        /** @type {Map<string, string[]>} parent → children (reverse of deps) */
        this._children = new Map();
    }

    // ── Public API ──────────────────────────────────────────────

    /**
     * Define (or redefine) a node.
     *
     * @param {string}     name
     * @param {string[]}  [deps]    — names of dependency nodes
     * @param {Function}  [compute] — async ({dep1, dep2, …}) => value
     */
    define(name, deps, compute) {
        const existing = this._nodes.get(name);
        this._nodes.set(name, {
            deps: deps || [],
            compute: compute || null,
            value: existing ? existing.value : undefined,
            subscribers: existing ? existing.subscribers : [],
        });
        for (const d of (deps || [])) {
            if (!this._children.has(d)) this._children.set(d, []);
            const ch = this._children.get(d);
            if (!ch.includes(name)) ch.push(name);
        }
    }

    /**
     * Subscribe to a node's value changes.
     *
     * @param {string}   name
     * @param {Function} fn — (value) => void
     * @returns {Function} unsubscribe
     */
    subscribe(name, fn) {
        let node = this._nodes.get(name);
        if (!node) {
            node = {deps: [], compute: null, value: undefined, subscribers: []};
            this._nodes.set(name, node);
        }
        node.subscribers.push(fn);
        return function () {
            var idx = node.subscribers.indexOf(fn);
            if (idx >= 0) node.subscribers.splice(idx, 1);
        };
    }

    /** Get current value of a node. */
    get(name) {
        var node = this._nodes.get(name);
        return node ? node.value : undefined;
    }

    /**
     * Collect all node values whose names do NOT start with '_'.
     * Useful for building a broadcast payload.
     *
     * @returns {Object}
     */
    getAll() {
        var result = {};
        for (var entry of this._nodes) {
            var n = entry[0], node = entry[1];
            if (n.charAt(0) !== '_') result[n] = node.value;
        }
        return result;
    }

    /**
     * Set a single source node's value and cascade.
     *
     * @param {string} name
     * @param {*}      value
     */
    async set(name, value) {
        var node = this._ensureNode(name);
        node.value = value;
        await this._notify(node);
        await this._propagate(name);
    }

    /**
     * Set several source values then run a **single** propagation pass.
     * Avoids redundant recomputation when many roots change at once.
     *
     * @param {Array<[string, *]>} entries — [[name, value], …]
     */
    async setMany(entries) {
        var changed = [];
        for (var i = 0; i < entries.length; i++) {
            var name = entries[i][0], value = entries[i][1];
            var node = this._ensureNode(name);
            node.value = value;
            changed.push(name);
        }

        // Notify source subscribers
        for (var ci = 0; ci < changed.length; ci++) {
            var srcNode = this._nodes.get(changed[ci]);
            await this._notify(srcNode);
        }

        // Collect all reachable descendants from every changed source
        var reachable = new Set();
        for (var ri = 0; ri < changed.length; ri++) {
            this._collectReachable(changed[ri], reachable);
        }
        if (reachable.size === 0) return;

        await this._processInOrder(reachable);
    }

    // ── Internals ───────────────────────────────────────────────

    /** Ensure a node exists, return it. */
    _ensureNode(name) {
        var node = this._nodes.get(name);
        if (!node) {
            node = {deps: [], compute: null, value: undefined, subscribers: []};
            this._nodes.set(name, node);
        }
        return node;
    }

    /** Fire all subscribers of a node. */
    async _notify(node) {
        for (var i = 0; i < node.subscribers.length; i++) {
            try { await node.subscribers[i](node.value); }
            catch (e) { console.error('[ReactiveGraph] subscriber error:', e); }
        }
    }

    /** Cascade changes from startName to all transitive dependents. */
    async _propagate(startName) {
        var reachable = new Set();
        this._collectReachable(startName, reachable);
        if (reachable.size === 0) return;
        await this._processInOrder(reachable);
    }

    /** Collect all transitive children of `name` into `set`. */
    _collectReachable(name, set) {
        var ch = this._children.get(name);
        if (!ch) return;
        for (var i = 0; i < ch.length; i++) {
            if (!set.has(ch[i])) {
                set.add(ch[i]);
                this._collectReachable(ch[i], set);
            }
        }
    }

    /**
     * Topological sort (DFS post-order, reversed) then process:
     * recompute each node and fire subscribers.
     */
    async _processInOrder(reachable) {
        var visited = new Set();
        var sorted = [];
        var self = this;

        function dfs(n) {
            if (visited.has(n)) return;
            visited.add(n);
            var ch = self._children.get(n);
            if (ch) {
                for (var i = 0; i < ch.length; i++) {
                    if (reachable.has(ch[i])) dfs(ch[i]);
                }
            }
            sorted.push(n);
        }

        for (var name of reachable) dfs(name);
        sorted.reverse(); // topological order: parents before children

        for (var si = 0; si < sorted.length; si++) {
            var nodeName = sorted[si];
            var node = this._nodes.get(nodeName);
            if (!node) continue;

            if (node.compute) {
                var depValues = {};
                for (var di = 0; di < node.deps.length; di++) {
                    var depNode = this._nodes.get(node.deps[di]);
                    depValues[node.deps[di]] = depNode ? depNode.value : undefined;
                }
                try {
                    node.value = await node.compute(depValues);
                } catch (e) {
                    console.error('[ReactiveGraph] compute error (' + nodeName + '):', e);
                    node.value = undefined;
                }
            }
            await this._notify(node);
        }
    }
}

// ── Universal export (CommonJS + browser) ───────────────────────
if (typeof module !== 'undefined' && module.exports) {
    module.exports = {ReactiveGraph};
} else if (typeof window !== 'undefined') {
    window.ReactiveGraph = ReactiveGraph;
}

