use std::collections::HashSet;

use crate::analyzer::domain_values::RuleViolationType;
use crate::analyzer::entities::RuleViolation;
use crate::analyzer::services::AccessRule;
use crate::parser::entities::ModuleNode;
use crate::parser::materials::ModuleTree;

pub struct Architecture<'r> {
    layer_names: HashSet<String>,
    access_rules: Vec<Box<dyn AccessRule + 'r>>,
}

impl<'r> Architecture<'r> {
    pub fn new(layer_names: HashSet<String>) -> Self {
        Architecture {
            layer_names,
            access_rules: Vec::default(),
        }
    }

    pub fn with_access_rule(mut self, access_rule: impl AccessRule + 'r) -> Self {
        self.access_rules.push(Box::new(access_rule));
        self
    }

    pub fn validate_access_rules(&'r self) -> Result<(), RuleViolation> {
        for access_rule in self.access_rules.iter() {
            if !access_rule.validate(&self.layer_names) {
                return Err(RuleViolation::new(RuleViolationType::LayerDoNotExist, Box::new(access_rule.clone()), vec![]));
            }
        }
        Ok(())
    }

    pub fn check_access_rules(&self, module_tree: &ModuleTree) -> Result<(), RuleViolation> {
        for access_rule in self.access_rules.iter() {
            access_rule.check(&module_tree)?;
        }
        Ok(())
    }

    pub fn check_complete_layer_specification(&self, module_tree: &ModuleTree) -> Result<(), RuleViolation> {
        let tree: &Vec<ModuleNode> = module_tree.tree();
        if tree.iter().any(|node| node.parent_index().is_some() && !self.layer_names.contains(node.module_name())
            && !self.layer_names.contains(tree[node.parent_index().unwrap()].module_name())) {
            return Err(RuleViolation::new(RuleViolationType::IncompleteLayerSpecification, Box::new(()), vec![]));
        }
        Ok(())
    }
}