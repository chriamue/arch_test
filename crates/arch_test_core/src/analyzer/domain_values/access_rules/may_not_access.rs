use std::collections::HashSet;

/// # `Accessor` may not access `accessed` relation
/// This access rule relation states that the `accessor` layer may not access the specified `accessed` layers.
/// As layer name it attempts to match either the module name or the parent module name, which is the directory the files were placed in.
/// If `when_same_parent` is `true`, the access rule is only applied within the same scope of modules that share the same parent.
#[derive(Debug, Clone)]
pub struct MayNotAccess {
    accessor: String,
    accessed: HashSet<String>,
    when_same_parent: bool,
}

impl MayNotAccess {
    pub fn new(accessor: String, accessed_layers: HashSet<String>, when_same_parent: bool) -> Self {
        MayNotAccess {
            accessor,
            accessed: accessed_layers,
            when_same_parent,
        }
    }

    pub fn accessor(&self) -> &String {
        &self.accessor
    }

    pub fn accessed(&self) -> &HashSet<String> {
        &self.accessed
    }

    pub fn when_same_parent(&self) -> bool {
        self.when_same_parent
    }
}
