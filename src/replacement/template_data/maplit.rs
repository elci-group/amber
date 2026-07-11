//! amber_maplit — Replacement for the `maplit` crate
//!
/// Use these macros for literal collection initialization.

/// Create a HashMap from key-value pairs
#[macro_export]
macro_rules! hashmap {
    ($($key:expr => $value:expr),* $(,)?) => {{
        let mut _map = ::std::collections::HashMap::new();
        $(_map.insert($key, $value);)*
        _map
    }};
}

/// Create a HashSet from elements
#[macro_export]
macro_rules! hashset {
    ($($item:expr),* $(,)?) => {{
        let mut _set = ::std::collections::HashSet::new();
        $(_set.insert($item);)*
        _set
    }};
}

/// Create a BTreeMap from key-value pairs
#[macro_export]
macro_rules! btreemap {
    ($($key:expr => $value:expr),* $(,)?) => {{
        let mut _map = ::std::collections::BTreeMap::new();
        $(_map.insert($key, $value);)*
        _map
    }};
}

/// Create a BTreeSet from elements
#[macro_export]
macro_rules! btreeset {
    ($($item:expr),* $(,)?) => {{
        let mut _set = ::std::collections::BTreeSet::new();
        $(_set.insert($item);)*
        _set
    }};
}

/// Create a Vec
#[macro_export]
macro_rules! vec_expr {
    ($($item:expr),* $(,)?) => {
        vec![$($item),*]
    };
}

