pub(crate) mod call_graph;
pub(crate) mod dfs_node;
pub(crate) mod import_graph;
pub(crate) mod ref_cache;
pub(crate) mod roper;
pub(crate) mod scope_resolver;
pub(crate) mod symbol_cache;
pub(crate) mod uri_lock;
pub(crate) mod uri_map;

#[cfg(test)]
mod import_graph_test;
#[cfg(test)]
mod scope_resolver_test;

