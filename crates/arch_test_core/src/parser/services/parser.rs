use std::fs::DirEntry;
use std::path::Path;

use ra_ap_syntax::{SourceFile, SyntaxKind, SyntaxNode, SyntaxNodeChildren, TextRange, TextSize};

use crate::parser::domain_values::{ObjectType, UsableObject};
use crate::parser::entities::ModuleNode;
use crate::parser::utils::read_file_content;

pub fn parse_main_or_mod_file_into_tree(
    tree: &mut Vec<ModuleNode>,
    file_path: &Path,
    level: usize,
    parent_index: Option<usize>,
    module_name: String,
) {
    let mut module_references: Vec<(usize, String)> = Vec::new();

    let result = SourceFile::parse(&read_file_content(file_path));
    parse_syntax_node_tree(
        tree,
        result.syntax_node().children(),
        file_path.to_str().unwrap().to_string(),
        level,
        parent_index,
        module_name,
        &mut module_references,
    );

    let dir_entries: Vec<DirEntry> = file_path
        .parent()
        .unwrap()
        .read_dir()
        .unwrap()
        .filter_map(|entry| entry.ok())
        .collect();
    for (parent_index, sub_module) in module_references {
        if let Some(entry) = dir_entries.iter().find(|entry| {
            entry
                .file_name()
                .to_str()
                .unwrap()
                .to_string()
                .trim_end_matches(".rs")
                .ends_with(&sub_module)
        }) {
            if entry.path().is_dir() {
                let path_str = format!("{}/mod.rs", entry.path().to_str().unwrap().to_string());
                parse_main_or_mod_file_into_tree(
                    tree,
                    Path::new(&path_str),
                    tree[parent_index].level() + 1,
                    Some(parent_index),
                    sub_module,
                );
            } else {
                parse_main_or_mod_file_into_tree(
                    tree,
                    &entry.path(),
                    tree[parent_index].level() + 1,
                    Some(parent_index),
                    sub_module,
                );
            }
        }
    }
}

fn parse_syntax_node_tree(
    tree: &mut Vec<ModuleNode>,
    syntax_node_children: SyntaxNodeChildren,
    file_path: String,
    level: usize,
    parent_index: Option<usize>,
    module_name: String,
    module_references: &mut Vec<(usize, String)>,
) {
    let current_index = tree.len();
    tree.push(ModuleNode::new(
        current_index,
        file_path.clone(),
        level,
        parent_index,
        module_name,
    ));
    if let Some(parent_index) = parent_index {
        tree.get_mut(parent_index)
            .unwrap()
            .register_child(current_index);
    }

    for item in syntax_node_children {
        if let Some((inner_module_start_node, inner_module_name)) = parse_file_rec(
            &item,
            module_references,
            &mut tree[current_index].usable_objects,
            current_index,
        ) {
            parse_syntax_node_tree(
                tree,
                inner_module_start_node,
                file_path.clone(),
                level + 1,
                Some(current_index),
                inner_module_name,
                module_references,
            );
        }
    }
}

// TODO: Generic types?
// TODO: Handle Path attribute and *
// TODO: Combined path types with custom impl, e.g. a::b::<c>::test(), where test was implemented by some trait in this crate
// TODO: Impl Self object use filtering?
fn parse_file_rec(
    syntax_node: &SyntaxNode,
    module_references: &mut Vec<(usize, String)>,
    usable_objects: &mut Vec<UsableObject>,
    current_index: usize,
) -> Option<(SyntaxNodeChildren, String)> {
    match syntax_node.kind() {
        SyntaxKind::USE => {
            let (is_pub, paths) = parse_use_paths(syntax_node);
            for (path, text_range) in paths {
                usable_objects.push(UsableObject::new(
                    is_pub,
                    if is_pub {
                        ObjectType::RePublish
                    } else {
                        ObjectType::Use
                    },
                    path,
                    text_range,
                ));
            }
        }
        SyntaxKind::STRUCT => {
            let mut is_pub = false;
            for child in syntax_node.children() {
                match child.kind() {
                    SyntaxKind::VISIBILITY => {
                        is_pub = true;
                    }
                    SyntaxKind::NAME => {
                        usable_objects.push(UsableObject::new(
                            is_pub,
                            ObjectType::Struct,
                            child.to_string(),
                            child.text_range(),
                        ));
                    }
                    SyntaxKind::RECORD_FIELD_LIST => {
                        for (impl_use_path, text_range) in parse_field_list(&child) {
                            usable_objects.push(UsableObject::new(
                                is_pub,
                                ObjectType::ImplicitUse,
                                impl_use_path,
                                text_range,
                            ));
                        }
                    }
                    _ => {
                        continue;
                    }
                }
            }
        }
        SyntaxKind::ENUM => {
            let mut is_pub = false;
            for child in syntax_node.children() {
                match child.kind() {
                    SyntaxKind::VISIBILITY => {
                        is_pub = true;
                    }
                    SyntaxKind::NAME => {
                        usable_objects.push(UsableObject::new(
                            is_pub,
                            ObjectType::Enum,
                            child.to_string(),
                            child.text_range(),
                        ));
                    }
                    SyntaxKind::VARIANT_LIST => {
                        for variant in child.children() {
                            for arg in variant.children() {
                                match arg.kind() {
                                    SyntaxKind::TUPLE_FIELD_LIST
                                    | SyntaxKind::RECORD_FIELD_LIST => {
                                        for (impl_use_path, text_range) in parse_field_list(&arg) {
                                            usable_objects.push(UsableObject::new(
                                                is_pub,
                                                ObjectType::ImplicitUse,
                                                impl_use_path,
                                                text_range,
                                            ));
                                        }
                                    }
                                    _ => continue,
                                }
                            }
                        }
                    }
                    _ => continue,
                }
            }
        }
        SyntaxKind::FN => {
            let mut is_pub = false;
            for child in syntax_node.children() {
                match child.kind() {
                    SyntaxKind::VISIBILITY => {
                        is_pub = true;
                    }
                    SyntaxKind::NAME => {
                        usable_objects.push(UsableObject::new(
                            is_pub,
                            ObjectType::Function,
                            child.to_string(),
                            child.text_range(),
                        ));
                    }
                    SyntaxKind::PARAM_LIST => {
                        for (impl_use_path, text_range) in parse_field_list(&child) {
                            usable_objects.push(UsableObject::new(
                                is_pub,
                                ObjectType::ImplicitUse,
                                impl_use_path,
                                text_range,
                            ));
                        }
                    }
                    SyntaxKind::RET_TYPE => {
                        for ret in child.children() {
                            match ret.kind() {
                                SyntaxKind::PATH_TYPE => {
                                    for (impl_use_path, text_range) in parse_path_type(&ret) {
                                        usable_objects.push(UsableObject::new(
                                            is_pub,
                                            ObjectType::ImplicitUse,
                                            impl_use_path,
                                            text_range,
                                        ));
                                    }
                                }
                                _ => continue,
                            }
                        }
                    }
                    SyntaxKind::BLOCK_EXPR => {
                        parse_file_rec(&child, module_references, usable_objects, current_index);
                    }
                    _ => {
                        continue;
                    }
                }
            }
        }
        SyntaxKind::PATH_EXPR | SyntaxKind::TUPLE_STRUCT_PAT => {
            for (impl_use_path, text_range) in parse_path_type(&syntax_node) {
                usable_objects.push(UsableObject::new(
                    false,
                    ObjectType::ImplicitUse,
                    impl_use_path,
                    text_range,
                ));
            }
        }
        SyntaxKind::TRAIT => {
            let mut is_pub = false;
            for child in syntax_node.children() {
                match child.kind() {
                    SyntaxKind::VISIBILITY => {
                        is_pub = true;
                    }
                    SyntaxKind::NAME => {
                        usable_objects.push(UsableObject::new(
                            is_pub,
                            ObjectType::Trait,
                            child.to_string(),
                            child.text_range(),
                        ));
                    }
                    SyntaxKind::ASSOC_ITEM_LIST => {
                        for (impl_use_path, text_range) in parse_assoc_func_item_list(&child) {
                            usable_objects.push(UsableObject::new(
                                is_pub,
                                ObjectType::ImplicitUse,
                                impl_use_path,
                                text_range,
                            ));
                        }
                    }
                    _ => continue,
                }
            }
        }
        SyntaxKind::IMPL => {
            for child in syntax_node.children() {
                match child.kind() {
                    SyntaxKind::PATH_TYPE => {
                        for (impl_use_path, text_range) in parse_path_type(&child) {
                            usable_objects.push(UsableObject::new(
                                false,
                                ObjectType::ImplicitUse,
                                impl_use_path,
                                text_range,
                            ));
                        }
                    }
                    SyntaxKind::ASSOC_ITEM_LIST => {
                        // TODO: Properly handle assoc list for trait impl
                        for (impl_use_path, text_range) in parse_assoc_func_item_list(&child) {
                            usable_objects.push(UsableObject::new(
                                false,
                                ObjectType::ImplicitUse,
                                impl_use_path,
                                text_range,
                            ));
                        }
                    }
                    _ => continue,
                }
            }
        }
        SyntaxKind::MODULE => {
            for child in syntax_node.children() {
                match child.kind() {
                    SyntaxKind::NAME => {
                        module_references.push((current_index, child.to_string()));
                    }
                    SyntaxKind::ITEM_LIST => {
                        return Some((child.children(), module_references.pop().unwrap().1));
                    }
                    _ => continue,
                }
            }
        }
        SyntaxKind::PARAM_LIST => {
            for (impl_use_path, text_range) in parse_field_list(&syntax_node) {
                usable_objects.push(UsableObject::new(
                    false,
                    ObjectType::ImplicitUse,
                    impl_use_path,
                    text_range,
                ));
            }
        }
        SyntaxKind::TUPLE_TYPE | SyntaxKind::PATH_TYPE | SyntaxKind::TUPLE_PAT => {
            for (impl_use_path, text_range) in parse_nested_tuple_type(&syntax_node) {
                usable_objects.push(UsableObject::new(
                    false,
                    ObjectType::ImplicitUse,
                    impl_use_path,
                    text_range,
                ));
            }
        }
        SyntaxKind::MATCH_EXPR => {
            for child in syntax_node.children() {
                match child.kind() {
                    SyntaxKind::MATCH_ARM_LIST => {
                        for match_arm in child.children() {
                            for arm_item in match_arm.children() {
                                match arm_item.kind() {
                                    SyntaxKind::PATH_PAT
                                    | SyntaxKind::WILDCARD_PAT
                                    | SyntaxKind::OR_PAT
                                    | SyntaxKind::RECORD_PAT => {
                                        // We wont need those special cases
                                        continue;
                                    }
                                    _ => {
                                        parse_file_rec(
                                            &arm_item,
                                            module_references,
                                            usable_objects,
                                            current_index,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => continue,
                }
            }
        }
        SyntaxKind::IDENT_PAT
        | SyntaxKind::RECORD_PAT
        | SyntaxKind::LITERAL
        | SyntaxKind::EXTERN_CRATE
        | SyntaxKind::CONTINUE_EXPR
        | SyntaxKind::BREAK_EXPR => {
            return None;
        }
        SyntaxKind::MACRO_CALL => {
            // TODO: Handle properly
            return None;
        }
        SyntaxKind::ATTR => {
            // TODO: Path Attribute!
            return None;
        }
        SyntaxKind::NAME_REF
        | SyntaxKind::REF_TYPE
        | SyntaxKind::RANGE_EXPR
        | SyntaxKind::FIELD_EXPR
        | SyntaxKind::BLOCK_EXPR
        | SyntaxKind::LET_STMT
        | SyntaxKind::BIN_EXPR
        | SyntaxKind::TUPLE_EXPR
        | SyntaxKind::PAREN_EXPR
        | SyntaxKind::METHOD_CALL_EXPR
        | SyntaxKind::CALL_EXPR
        | SyntaxKind::CLOSURE_EXPR
        | SyntaxKind::PREFIX_EXPR
        | SyntaxKind::REF_EXPR
        | SyntaxKind::IF_EXPR
        | SyntaxKind::FOR_EXPR
        | SyntaxKind::WHILE_EXPR
        | SyntaxKind::RETURN_EXPR
        | SyntaxKind::INDEX_EXPR
        | SyntaxKind::CAST_EXPR
        | SyntaxKind::TRY_EXPR
        | SyntaxKind::CONDITION
        | SyntaxKind::ARG_LIST
        | SyntaxKind::EXPR_STMT => {
            for child in syntax_node.children() {
                parse_file_rec(&child, module_references, usable_objects, current_index);
            }
        }
        _ => {
            println!(
                "UNHANDLED EXPRESSION: {:?} => {}",
                syntax_node,
                syntax_node.to_string()
            );
            return None;
        }
    }
    None
}

fn parse_use_paths(syntax_node: &SyntaxNode) -> (bool, Vec<(String, TextRange)>) {
    let mut visibility = false;
    let mut paths = Vec::new();
    for child in syntax_node.children() {
        match child.kind() {
            SyntaxKind::VISIBILITY => {
                visibility = true;
            }
            SyntaxKind::USE_TREE => {
                if child.to_string().ends_with('*') {
                    paths.push((child.to_string(), child.text_range()));
                } else {
                    paths.append(&mut parse_use_tree(&child));
                }
            }
            _ => unreachable!(),
        }
    }
    (visibility, paths)
}

fn parse_use_tree(syntax_node: &SyntaxNode) -> Vec<(String, TextRange)> {
    let mut path_segments = Vec::new();
    let mut current_prefix = String::new();
    let mut current_text_range = TextRange::empty(TextSize::default());
    for sub_child in syntax_node.children() {
        match sub_child.kind() {
            SyntaxKind::PATH => {
                current_prefix = sub_child.to_string();
                current_text_range = sub_child.text_range();
            }
            SyntaxKind::USE_TREE_LIST => {
                for use_tree in sub_child.children() {
                    for (segment, _) in parse_use_tree(&use_tree) {
                        path_segments.push((
                            format!("{}::{}", current_prefix, segment),
                            sub_child.text_range(),
                        ));
                    }
                }
            }
            _ => unreachable!(),
        }
    }
    if path_segments.is_empty() {
        return vec![(current_prefix, current_text_range)];
    }
    path_segments
}

fn parse_path_type(syntax_node: &SyntaxNode) -> Vec<(String, TextRange)> {
    let mut obj_uses = Vec::new();
    let mut current_path = String::new();
    for path_child in syntax_node.children() {
        match path_child.kind() {
            SyntaxKind::PATH => {
                for i_path_child in path_child.children() {
                    match i_path_child.kind() {
                        SyntaxKind::PATH => {
                            current_path = i_path_child.to_string();
                        }
                        SyntaxKind::PATH_SEGMENT => {
                            for p_segment_child in i_path_child.children() {
                                match p_segment_child.kind() {
                                    SyntaxKind::NAME_REF => {
                                        if current_path.is_empty() {
                                            obj_uses.push((
                                                p_segment_child.to_string(),
                                                p_segment_child.text_range(),
                                            ));
                                        } else {
                                            obj_uses.push((
                                                format!(
                                                    "{}::{}",
                                                    current_path,
                                                    p_segment_child.to_string()
                                                ),
                                                p_segment_child.text_range(),
                                            ));
                                        }
                                    }
                                    SyntaxKind::GENERIC_ARG_LIST => {
                                        for arg in p_segment_child.children() {
                                            match arg.kind() {
                                                SyntaxKind::TYPE_ARG => {
                                                    for t_arg_child in arg.children() {
                                                        match t_arg_child.kind() {
                                                            SyntaxKind::PATH_TYPE
                                                            | SyntaxKind::TUPLE_TYPE => {
                                                                obj_uses.append(
                                                                    &mut parse_nested_tuple_type(
                                                                        &t_arg_child,
                                                                    ),
                                                                );
                                                            }
                                                            _ => continue,
                                                        }
                                                    }
                                                }
                                                _ => continue,
                                            }
                                        }
                                    }
                                    _ => continue,
                                }
                            }
                        }
                        _ => continue,
                    }
                }
            }
            _ => continue,
        }
    }

    obj_uses
}

fn parse_field_list(syntax_node: &SyntaxNode) -> Vec<(String, TextRange)> {
    let mut result = Vec::new();
    for rfl_child in syntax_node.children() {
        for rf_child in rfl_child.children() {
            result.append(&mut parse_nested_tuple_type(&rf_child));
        }
    }
    result
}

fn parse_nested_tuple_type(syntax_node: &SyntaxNode) -> Vec<(String, TextRange)> {
    let mut result = Vec::new();
    match syntax_node.kind() {
        SyntaxKind::NAME
        | SyntaxKind::IDENT_PAT
        | SyntaxKind::WILDCARD_PAT
        | SyntaxKind::LIFETIME
        | SyntaxKind::VISIBILITY
        | SyntaxKind::ATTR => {
            return result;
        }
        SyntaxKind::TUPLE_TYPE
        | SyntaxKind::SLICE_TYPE
        | SyntaxKind::PAREN_TYPE
        | SyntaxKind::REF_TYPE
        | SyntaxKind::TUPLE_PAT
        | SyntaxKind::IMPL_TRAIT_TYPE
        | SyntaxKind::TYPE_BOUND_LIST
        | SyntaxKind::TYPE_BOUND => {
            for child in syntax_node.children() {
                result.append(&mut parse_nested_tuple_type(&child));
            }
        }
        SyntaxKind::PATH_TYPE => {
            result.append(&mut parse_path_type(&syntax_node));
        }
        _ => {
            println!("{:?} => {}", syntax_node, syntax_node.to_string());
            unreachable!()
        }
    }
    result
}

fn parse_assoc_func_item_list(syntax_node: &SyntaxNode) -> Vec<(String, TextRange)> {
    let mut result = Vec::new();
    for arg in syntax_node.children() {
        for func in arg.children() {
            match func.kind() {
                SyntaxKind::PARAM_LIST => {
                    result.append(&mut parse_field_list(&func));
                }
                SyntaxKind::RET_TYPE => {
                    for ret in func.children() {
                        match ret.kind() {
                            SyntaxKind::PATH_TYPE => {
                                result.append(&mut parse_path_type(&ret));
                            }
                            _ => continue,
                        }
                    }
                }
                _ => continue,
            }
        }
    }
    result
}
